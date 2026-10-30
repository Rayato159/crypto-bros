use crypto_bros::{
    chain::Balances,
    live::{LiveConfig, RealAccount, RealLayer, SOL, ThbRate, USDC},
    state::Snapshot,
    storage::Store,
    watch,
};
use crypto_bros_core::{model::ForecastModel, research::demo_candles};

fn fixture() -> Snapshot {
    let now = chrono::Utc::now().timestamp();
    let mut s = Snapshot::new().unwrap();
    s.source = "binance_solusdc".into();
    s.price = 100.0;
    s.updated_at = now;
    s.candles = demo_candles();
    s.revalue().unwrap();
    let rate = ThbRate {
        micros: 35_000_000,
        fetched_at: now,
        source: "isolated test".into(),
    };
    let mut a = RealAccount::new(
        "synthetic-unfunded-address".into(),
        100 * USDC,
        200 * USDC,
        500_000,
        true,
        LiveConfig {
            trading_budget: 60 * USDC,
            max_buy: 30 * USDC,
            reserve: 40 * USDC,
            max_layers: 2,
            sol_gas_reserve: 5_000_000,
            max_network_fee: 1_000_000,
            slippage_bps: 50,
        },
        &rate,
        now,
    )
    .unwrap();
    a.advice_model = Some(ForecastModel {
        version: "isolated-fixture".into(),
        weights: [0.05, 0.0, 0.0, 0.0, 0.0, 0.0],
        means: [0.0; 5],
        scales: [1.0; 5],
        uncertainty: 0.001,
        trained_samples: 200,
        training_end: 0,
    });
    s.real = Some(a);
    s.thb_rate = Some(rate);
    s.real_balances = Some(Balances {
        sol: 10_000_000,
        usdc: 100 * USDC,
        wrapped_sol: 0,
        rent: 0,
        checked_at: now,
    });
    s.live_enabled = true;
    s.watch.enabled = true;
    s
}
fn layer(s: &mut Snapshot, id: u64) {
    let a = s.real.as_mut().unwrap();
    a.cash -= 10 * USDC;
    a.layers.push(RealLayer {
        id,
        quantity: SOL / 10,
        cost_usdc: 10 * USDC,
        cost_satang: 35_000,
        entry_fee_usdc: 10_000,
        entry_fee_satang: 35,
        target_price: 110 * USDC,
        review_price: 90 * USDC,
        opened_at: 0,
        model: "fixture".into(),
    });
    let b = s.real_balances.as_mut().unwrap();
    b.sol += SOL / 10;
    b.usdc = a.cash;
}
fn cards(s: &Snapshot) -> Vec<serde_json::Value> {
    watch::view(s, chrono::Utc::now().timestamp())["signals"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn buy_allocates_only_available_budget_and_validates_model() {
    let mut s = fixture();
    let c = cards(&s);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["amount_usdc"], 30 * USDC);
    let p = watch::order_payload(&s, c[0]["key"].as_str().unwrap(), s.updated_at).unwrap();
    assert_eq!(p["amount_usdc"], "30.000000");
    assert!(p["manual"].is_null());
    s.real.as_mut().unwrap().parked_profit = 50 * USDC;
    assert_eq!(cards(&s)[0]["amount_usdc"], 10 * USDC);
    s.real
        .as_mut()
        .unwrap()
        .advice_model
        .as_mut()
        .unwrap()
        .weights[0] = 0.0;
    assert!(cards(&s).is_empty());
}
#[test]
fn full_layers_do_not_propose_a_buy_even_with_a_positive_model() {
    let mut s = fixture();
    layer(&mut s, 1);
    layer(&mut s, 2);
    assert_eq!(watch::buy_size(s.real.as_ref().unwrap()), 0);
    assert!(cards(&s).is_empty());
}
#[test]
fn sells_continue_with_new_buys_paused_and_use_one_layer_only() {
    let mut s = fixture();
    layer(&mut s, 1);
    s.price = 112.0;
    s.live_enabled = false;
    let c = cards(&s);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["side"], "sell");
    assert_eq!(c[0]["risk"], false);
    assert_eq!(c[0]["pnl_usdc"], 1_190_000);
    let p = watch::order_payload(&s, c[0]["key"].as_str().unwrap(), s.updated_at).unwrap();
    assert_eq!(p["layer_id"], 1);
    assert_eq!(p["percent"], 100);
}
#[test]
fn risk_exit_wins_over_new_buys() {
    let mut s = fixture();
    layer(&mut s, 1);
    s.price = 89.0;
    let c = cards(&s);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["risk"], true);
    assert_eq!(c[0]["side"], "sell");
}
#[test]
fn target_exit_must_clear_entry_and_exit_costs_at_the_quote_minimum() {
    let mut s = fixture();
    layer(&mut s, 1);
    let l = &s.real.as_ref().unwrap().layers[0];
    assert!(!watch::profit_exit_covers_costs(
        l,
        10_050_000,
        1_000_000,
        100 * USDC
    ));
    assert!(watch::profit_exit_covers_costs(
        l,
        11_000_000,
        1_000_000,
        100 * USDC
    ));
}
#[test]
fn stale_prices_fx_balances_and_synthetic_data_never_create_trade_cards() {
    for field in 0..5 {
        let mut s = fixture();
        match field {
            0 => s.updated_at -= 121,
            1 => s.thb_rate.as_mut().unwrap().fetched_at -= 601,
            2 => s.real_balances.as_mut().unwrap().checked_at -= 121,
            3 => s.source = "synthetic_demo".into(),
            _ => s.market_error = Some("offline".into()),
        }
        assert!(cards(&s).is_empty(), "field {field}");
    }
}
#[test]
fn missing_gas_or_withdrawn_cash_blocks_buy_cards() {
    let mut s = fixture();
    s.real_balances.as_mut().unwrap().sol = 5_000_000;
    assert!(cards(&s).is_empty());
    s.real_balances.as_mut().unwrap().sol = 10_000_000;
    s.real_balances.as_mut().unwrap().usdc = 99 * USDC;
    assert!(cards(&s).is_empty());
}
#[test]
fn stale_clicked_card_cannot_prepare_after_signal_changes_or_pause() {
    let mut s = fixture();
    let key = cards(&s)[0]["key"].as_str().unwrap().to_owned();
    s.live_enabled = false;
    assert!(watch::order_payload(&s, &key, s.updated_at).is_err());
    s.live_enabled = true;
    s.real
        .as_mut()
        .unwrap()
        .advice_model
        .as_mut()
        .unwrap()
        .weights[0] = 0.0;
    assert!(watch::order_payload(&s, &key, s.updated_at).is_err());
    let mut s = fixture();
    layer(&mut s, 1);
    s.price = 112.0;
    let key = cards(&s)[0]["key"].as_str().unwrap().to_owned();
    s.price = 100.0;
    assert!(watch::order_payload(&s, &key, s.updated_at).is_err());
}
#[test]
fn duplicate_notifications_and_snoozes_survive_serialization() {
    let mut s = fixture();
    let now = s.updated_at;
    let first = watch::claim(&mut s, now).unwrap();
    assert!(watch::claim(&mut s, now).is_none());
    let mut loaded: Snapshot = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert!(watch::claim(&mut loaded, now).is_none());
    let key = first["key"].as_str().unwrap().to_owned();
    loaded
        .watch
        .notified
        .insert(key.clone(), now - watch::REMINDER_SECONDS);
    assert!(watch::claim(&mut loaded, now).is_some());
    loaded
        .watch
        .snoozed
        .insert(key.clone(), now + watch::REMINDER_SECONDS);
    assert!(cards(&loaded).is_empty());
    assert!(watch::order_payload(&loaded, &key, now).is_err());
    loaded.watch.snoozed.insert(key, now - 1);
    assert_eq!(cards(&loaded).len(), 1);
}
#[test]
fn restart_disables_monitoring_and_preserves_the_funded_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.sqlite")).unwrap();
    let s = fixture();
    store.save(&s, "fixture").unwrap();
    let loaded = store.load().unwrap();
    assert!(!loaded.watch.enabled);
    assert!(!loaded.live_enabled);
    assert_eq!(loaded.real.unwrap().cash, 100 * USDC);
    let mut old = serde_json::to_value(s).unwrap();
    old.as_object_mut().unwrap().remove("watch");
    let loaded: Snapshot = serde_json::from_value(old).unwrap();
    assert!(!loaded.watch.enabled);
}

fn service(dir: &std::path::Path) -> crypto_bros::api::AppState {
    use std::sync::Arc;
    use tokio::sync::{Mutex, Notify};
    let store = Store::open(&dir.join("service-test.sqlite")).unwrap();
    crypto_bros::api::AppState {
        engine: Arc::new(Mutex::new(crypto_bros::api::Engine {
            snapshot: fixture(),
            store,
            pair_ticket: None,
            live_ticket: None,
        })),
        token: "isolated-service-token".into(),
        client: reqwest::Client::new(),
        shutdown: Arc::new(Notify::new()),
        data_dir: dir.into(),
        port: 0,
        live_guard: Arc::new(Mutex::new(())),
    }
}
async fn request(
    state: &crypto_bros::api::AppState,
    action: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = crypto_bros::api::router(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let response = state
        .client
        .post(format!("http://127.0.0.1:{port}/api/command"))
        .bearer_auth(&state.token)
        .json(&serde_json::json!({"action":action,"payload":payload}))
        .send()
        .await
        .unwrap();
    let success = response.status().is_success();
    let body: serde_json::Value = response.json().await.unwrap();
    task.abort();
    if success {
        Ok(body)
    } else {
        Err(body["error"].as_str().unwrap().into())
    }
}
#[tokio::test]
async fn concurrent_desktop_claims_notify_once_without_creating_orders() {
    let dir = tempfile::tempdir().unwrap();
    let state = service(dir.path());
    let (a, b) = tokio::join!(
        request(&state, "live_watch_claim", serde_json::json!({})),
        request(&state, "live_watch_claim", serde_json::json!({}))
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_ne!(a["signal"].is_null(), b["signal"].is_null());
    let e = state.engine.lock().await;
    assert!(e.snapshot.real.as_ref().unwrap().pending.is_none());
    assert!(e.snapshot.real.as_ref().unwrap().fills.is_empty());
}
#[tokio::test]
async fn stale_notification_after_stop_is_rejected_before_quote_access() {
    let dir = tempfile::tempdir().unwrap();
    let state = service(dir.path());
    let signal = request(&state, "live_watch_claim", serde_json::json!({}))
        .await
        .unwrap();
    request(&state, "live_watch_stop", serde_json::json!({}))
        .await
        .unwrap();
    let result = request(
        &state,
        "live_watch_prepare",
        serde_json::json!({"key":signal["signal"]["key"]}),
    )
    .await;
    assert!(result.unwrap_err().contains("สัญญาณนี้"));
    let e = state.engine.lock().await;
    assert!(!e.snapshot.watch.enabled);
    assert!(!e.snapshot.live_enabled);
    assert!(e.snapshot.real.as_ref().unwrap().pending.is_none());
}

#[test]
fn external_gas_deposits_change_wallet_value_but_never_bot_profit_or_limits() {
    let mut s = fixture();
    let now = chrono::Utc::now().timestamp();
    let before = crypto_bros::live_api::advice(&s)["bot_pnl_usdc"].clone();
    let initial = crypto_bros::readout::view(&s, now);
    s.real_balances.as_mut().unwrap().sol += SOL;
    let after = crypto_bros::readout::view(&s, now);
    assert_eq!(
        after["wallet_usdc"].as_u64().unwrap() - initial["wallet_usdc"].as_u64().unwrap(),
        100 * USDC
    );
    assert_eq!(crypto_bros::live_api::advice(&s)["bot_pnl_usdc"], before);
    assert_eq!(s.real.as_ref().unwrap().config.trading_budget, 60 * USDC);
}
#[test]
fn wallet_value_does_not_subtract_recorded_fees_twice_and_stale_marks_are_unavailable() {
    let mut s = fixture();
    let now = chrono::Utc::now().timestamp();
    s.real.as_mut().unwrap().fees_usdc = USDC;
    let b = s.real_balances.as_mut().unwrap();
    b.usdc = 100 * USDC;
    b.sol = SOL;
    let view = crypto_bros::readout::view(&s, now);
    assert_eq!(view["wallet_usdc"], 200 * USDC);
    assert_eq!(view["network_fee_usdc"], USDC);
    s.updated_at = now - 300;
    let view = crypto_bros::readout::view(&s, now);
    assert!(view["wallet_usdc"].is_null());
    assert_eq!(view["fresh"], false);
    assert_eq!(view["native_sol"], SOL);
}
