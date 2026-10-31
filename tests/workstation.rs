use crypto_bros::{
    api::{self, AppState, Engine},
    state::Snapshot,
    storage::Store,
};
use crypto_bros_core::{ledger::USDC, research::demo_candles};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

struct Harness {
    state: AppState,
    client: reqwest::Client,
    base: String,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Harness {
    async fn new() -> Self {
        Self::with_blocked_backup(false).await
    }
    async fn with_blocked_backup(blocked: bool) -> Self {
        let dir = tempfile::tempdir().expect("isolated directory");
        let store = Store::open(&dir.path().join("test.sqlite")).expect("store");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("address").port();
        let backup_dir = if blocked {
            let p = dir.path().join("not-a-directory");
            std::fs::write(&p, b"isolated backup failure fixture").unwrap();
            p
        } else {
            dir.path().into()
        };
        let state = AppState {
            engine: Arc::new(Mutex::new(Engine {
                snapshot: Snapshot::new().expect("state"),
                store,
                pair_ticket: None,
                live_ticket: None,
            })),
            token: "isolated-test-token-not-a-wallet-secret".into(),
            client: reqwest::Client::new(),
            shutdown: Arc::new(Notify::new()),
            data_dir: backup_dir,
            port,
            live_guard: Arc::new(Mutex::new(())),
        };
        let app = api::router(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        Self {
            state,
            client: reqwest::Client::new(),
            base: format!("http://127.0.0.1:{port}"),
            task,
            _dir: dir,
        }
    }
    async fn command(&self, action: &str, payload: Value) -> (u16, Value) {
        let response = self
            .client
            .post(format!("{}/api/command", self.base))
            .bearer_auth(&self.state.token)
            .json(&json!({"action":action,"payload":payload}))
            .send()
            .await
            .expect("response");
        (
            response.status().as_u16(),
            response.json().await.expect("json"),
        )
    }
    async fn ok(&self, action: &str, payload: Value) {
        let (status, body) = self.command(action, payload).await;
        assert_eq!(status, 200, "{body}");
    }
}

#[tokio::test]
async fn local_api_requires_token_and_rejects_foreign_origins() {
    let h = Harness::new().await;
    let url = format!("{}/api/state", h.base);
    assert_eq!(
        h.client.get(&url).send().await.expect("request").status(),
        401
    );
    assert_eq!(
        h.client
            .get(&url)
            .bearer_auth(&h.state.token)
            .header("origin", "https://evil.example")
            .send()
            .await
            .expect("request")
            .status(),
        401
    );
    assert_eq!(
        h.client
            .get(&url)
            .bearer_auth(&h.state.token)
            .send()
            .await
            .expect("request")
            .status(),
        200
    );
    assert_eq!(h.command("enable_live", json!({})).await.0, 400);
}

fn real_fixture() -> crypto_bros::live::RealAccount {
    use crypto_bros::live::{LiveConfig, RealAccount, ThbRate};
    RealAccount::new(
        crypto_bros::solana_wire::SYSTEM.into(),
        30_000_000,
        120_000_000,
        420_000,
        true,
        LiveConfig {
            trading_budget: 20_000_000,
            max_buy: 10_000_000,
            reserve: 10_000_000,
            max_layers: 2,
            sol_gas_reserve: 5_000_000,
            max_network_fee: 1_000_000,
            slippage_bps: 50,
        },
        &ThbRate {
            micros: 34_000_000,
            fetched_at: 0,
            source: "synthetic-test".into(),
        },
        0,
    )
    .unwrap()
}

#[tokio::test]
async fn moving_from_demo_preserves_the_complete_old_account_in_a_backup() {
    let h = Harness::new().await;
    h.ok("demo", json!({})).await;
    h.ok("buy", json!({})).await;
    let old_quantity = h.state.engine.lock().await.snapshot.ledger.layers[0].quantity;
    h.ok("archive_demo", json!({})).await;
    {
        let e = h.state.engine.lock().await;
        assert_eq!(e.snapshot.source, "not_loaded");
        assert!(e.snapshot.ledger.layers.is_empty());
        assert!(!e.snapshot.running);
    }
    let backup = std::fs::read_dir(h._dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("archived-demo-")
        })
        .unwrap();
    let archived = Store::open(&backup).unwrap().load().unwrap();
    assert_eq!(archived.ledger.layers[0].quantity, old_quantity);
    assert_eq!(archived.source, "synthetic_demo");
}
fn pending_fixture() -> crypto_bros::live::PendingOrder {
    crypto_bros::live::PendingOrder {
        entry_context: None,
        id: "synthetic-order".into(),
        side: "buy".into(),
        layer_id: 0,
        input: 1_000_000,
        expected_output: 10_000_000,
        minimum_output: 9_900_000,
        unsigned_transaction: String::new(),
        request_id: "synthetic".into(),
        last_valid_block_height: 1,
        prepared_at: 0,
        expires_at: chrono::Utc::now().timestamp() + 60,
        signature: None,
        status: "awaiting_approval".into(),
        network_fee: 5000,
        rent_delta: 0,
        rate: crypto_bros::live::ThbRate {
            micros: 34_000_000,
            fetched_at: 0,
            source: "synthetic-test".into(),
        },
        price: 100_000_000,
        target_price: 110_000_000,
        review_price: 90_000_000,
        model: "manual-live".into(),
        message: None,
    }
}
#[tokio::test]
async fn signed_orders_survive_restart_and_block_updates_or_cancellation() {
    let h = Harness::new().await;
    {
        let mut e = h.state.engine.lock().await;
        let mut account = real_fixture();
        let mut order = pending_fixture();
        order.signature = Some("synthetic-pending-signature".into());
        account.pending = Some(order);
        e.snapshot.real = Some(account);
        e.snapshot.live_enabled = true;
        let snapshot = e.snapshot.clone();
        e.store.save(&snapshot, "test_pending").unwrap();
        let reloaded = e.store.load().unwrap();
        assert!(!reloaded.live_enabled);
        assert!(reloaded.real.unwrap().pending.unwrap().signature.is_some());
    }
    assert_eq!(h.command("prepare_update", json!({})).await.0, 400);
    assert_eq!(h.command("live_cancel", json!({})).await.0, 400);
    h.ok("pause", json!({})).await;
    let e = h.state.engine.lock().await;
    assert!(e.snapshot.real.as_ref().unwrap().pending.is_some());
    assert!(!e.snapshot.live_enabled);
}
#[tokio::test]
async fn approval_is_origin_bound_expiring_and_invalidated_by_pause() {
    let h = Harness::new().await;
    {
        let mut e = h.state.engine.lock().await;
        let mut account = real_fixture();
        account.pending = Some(pending_fixture());
        account.reserve_bps = 2000;
        e.snapshot.real = Some(account);
        e.snapshot.live_enabled = true;
    }
    let (status, link) = h.command("live_link", json!({})).await;
    assert_eq!(status, 200);
    let ticket = link["url"].as_str().unwrap().split('#').nth(1).unwrap();
    let body = json!({"ticket":ticket});
    let url = format!("{}/live/preview", h.base);
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", "https://untrusted.example")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", &h.base)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    h.ok("live_pause", json!({})).await;
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", &h.base)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert!(
        h.state
            .engine
            .lock()
            .await
            .snapshot
            .real
            .as_ref()
            .unwrap()
            .pending
            .is_none()
    );
}
#[tokio::test]
async fn real_account_does_not_inherit_paper_money_and_cannot_change_wallet() {
    let h = Harness::new().await;
    {
        h.state.engine.lock().await.snapshot.real = Some(real_fixture());
    }
    let e = h.state.engine.lock().await;
    assert_eq!(e.snapshot.ledger.cash, 1_000_000_000);
    assert_eq!(e.snapshot.real.as_ref().unwrap().cash, 30_000_000);
    assert_eq!(
        e.snapshot.real.as_ref().unwrap().initial_cost_satang,
        105_000
    );
    drop(e);
    assert_eq!(
        h.command(
            "wallet_address",
            json!({"address":"So11111111111111111111111111111111111111112"})
        )
        .await
        .0,
        400
    );
    assert_eq!(h.command("demo", json!({})).await.0, 400);
}

#[tokio::test]
async fn layers_feedback_partial_sales_and_restart_preserve_accounting() {
    let h = Harness::new().await;
    h.ok("demo", json!({})).await;
    for _ in 0..4 {
        h.ok("buy", json!({})).await;
    }
    assert_eq!(h.command("buy", json!({})).await.0, 400);
    h.ok("feedback", json!({"decision_id":1,"rating":"too_risky"}))
        .await;
    h.ok("sell", json!({"id":1,"percent":50})).await;
    h.ok("sell", json!({"id":1,"percent":100})).await;
    let e = h.state.engine.lock().await;
    let s = &e.snapshot;
    assert!(s.ledger.cash >= 400 * USDC);
    assert_eq!(s.ledger.layers[0].remaining_cost, 0);
    assert_eq!(s.feedback.len(), 1);
    let saved = e.store.load().expect("reload");
    assert_eq!(saved.ledger.fills.len(), 6);
    assert!(!saved.running && !saved.live_enabled);
}

#[tokio::test]
async fn risk_acknowledgement_cannot_override_limits_or_simulate_liquidation() {
    let h = Harness::new().await;
    h.ok("demo", json!({})).await;
    for _ in 0..4 {
        h.ok("buy", json!({})).await;
    }
    {
        let mut e = h.state.engine.lock().await;
        e.snapshot.price *= 0.1;
        e.snapshot.running = true;
        e.snapshot.evaluate().expect("risk review");
        assert!(!e.snapshot.running);
        assert!(e.snapshot.net_return < -0.5);
        assert_eq!(
            e.snapshot.ledger.fills.len(),
            4,
            "No assumed stop-loss fills"
        );
    }
    h.ok("acknowledge", json!({})).await;
    assert_eq!(h.command("buy", json!({})).await.0, 400);
    h.ok("sell", json!({"id":1,"percent":100})).await;
}

#[tokio::test]
async fn candidate_requires_explicit_activation_and_fresh_holdout() {
    let h = Harness::new().await;
    h.ok("demo", json!({})).await;
    h.ok("research", json!({})).await;
    {
        let e = h.state.engine.lock().await;
        assert!(e.snapshot.active_model.is_none());
        assert!(!e.snapshot.running);
        assert!(e.snapshot.research.is_some());
    }
    assert_eq!(h.command("research", json!({})).await.0, 400);
    h.ok("activate_paper", json!({})).await;
    h.ok("start", json!({})).await;
    h.ok("pause", json!({})).await;
    assert!(!h.state.engine.lock().await.snapshot.running);
}

#[tokio::test]
async fn update_quiesces_and_creates_restorable_backup() {
    let h = Harness::new().await;
    h.ok("demo", json!({})).await;
    h.ok("buy", json!({})).await;
    h.ok("prepare_update", json!({})).await;
    assert_eq!(h.command("buy", json!({})).await.0, 400);
    assert_eq!(h.command("start", json!({})).await.0, 400);
    let backup = std::fs::read_dir(h._dir.path())
        .expect("dir")
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().starts_with("backup-"))
        .expect("backup");
    let saved = Store::open(&backup.path())
        .expect("open backup")
        .load()
        .expect("load");
    assert_eq!(saved.ledger.open_count(), 1);
    assert!(!saved.running && !saved.updating);
}

#[tokio::test]
async fn pairing_is_expiring_single_use_and_does_not_prove_wallet_ownership() {
    let h = Harness::new().await;
    let (_, link) = h.command("wallet_link", json!({})).await;
    let ticket = link["url"]
        .as_str()
        .expect("url")
        .split('#')
        .nth(1)
        .expect("ticket");
    // Public system program address only; no wallet keys or signing in this test.
    let payload = json!({"ticket":ticket,"address":"11111111111111111111111111111111"});
    let url = format!("{}/wallet/pair", h.base);
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", "https://evil.example")
            .json(&payload)
            .send()
            .await
            .expect("pair")
            .status(),
        403
    );
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", &h.base)
            .json(&payload)
            .send()
            .await
            .expect("pair")
            .status(),
        200
    );
    assert_eq!(
        h.client
            .post(&url)
            .header("origin", &h.base)
            .json(&payload)
            .send()
            .await
            .expect("pair")
            .status(),
        403
    );
    assert!(
        h.state
            .engine
            .lock()
            .await
            .snapshot
            .wallet
            .as_ref()
            .expect("wallet")
            .verified_at
            .is_none()
    );
    h.state.engine.lock().await.pair_ticket = Some(("expired-test-ticket".into(), 0));
    assert_eq!(h.client.post(&url).header("origin", &h.base).json(&json!({"ticket":"expired-test-ticket","address":"11111111111111111111111111111111"})).send().await.expect("pair").status(),403);
}

#[test]
fn stale_and_future_timestamps_never_authorize_a_paper_fill() {
    let mut s = Snapshot::new().expect("state");
    s.source = "binance_solusdc".into();
    s.candles = demo_candles();
    s.price = 150.0;
    s.updated_at = chrono::Utc::now().timestamp() - 121;
    assert!(s.paper_buy().is_err());
    s.updated_at = chrono::Utc::now().timestamp() + 120;
    assert!(s.paper_buy().is_err());
}

#[test]
fn private_directory_can_be_created_and_reopened_without_admin_privileges() {
    let dir = tempfile::tempdir().expect("isolated directory");
    let path = dir.path().join("private-data");
    crypto_bros_core::privacy::protect_directory(&path).expect("restrict access");
    std::fs::write(path.join("test.txt"), b"test only").expect("owner can write");
    crypto_bros_core::privacy::protect_directory(&path).expect("reopen on restart");
    assert_eq!(
        std::fs::read(path.join("test.txt")).expect("owner can read"),
        b"test only"
    );
}

#[tokio::test]
async fn failed_update_backup_keeps_worker_quiesced_and_blocks_resume() {
    let h = Harness::with_blocked_backup(true).await;
    h.ok("demo", json!({})).await;
    let (status, _) = h.command("prepare_update", json!({})).await;
    assert_eq!(status, 400);
    let e = h.state.engine.lock().await;
    assert!(e.snapshot.updating);
    assert!(!e.snapshot.running);
    assert!(!e.snapshot.live_enabled);
    assert!(!e.snapshot.watch.enabled);
    drop(e);
    assert_eq!(h.command("start", json!({})).await.0, 400);
}
