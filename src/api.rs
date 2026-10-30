use crate::{
    market,
    state::{Feedback, Snapshot, Wallet},
    storage::Store,
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, Response},
    routing::{get, post},
};
use crypto_bros_core::{
    indicators::Candle,
    ledger::{USDC, price_units},
    research::{demo_candles, optimize},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, Notify};

pub struct Engine {
    pub snapshot: Snapshot,
    pub store: Store,
    pub pair_ticket: Option<(String, i64)>,
    pub live_ticket: Option<(String, String, i64)>,
}
impl Engine {
    pub(crate) fn commit(&mut self, event: &str) -> Result<(), String> {
        if let Err(error) = self.store.save(&self.snapshot, event) {
            // Never continue from a fill that failed to persist.
            if let Ok(saved) = self.store.load() {
                self.snapshot = saved;
            }
            self.snapshot.running = false;
            self.snapshot.live_enabled = false;
            self.snapshot.watch.enabled = false;
            self.live_ticket = None;
            return Err(error);
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Mutex<Engine>>,
    pub token: String,
    pub client: reqwest::Client,
    pub shutdown: Arc<Notify>,
    pub data_dir: PathBuf,
    pub port: u16,
    pub live_guard: Arc<Mutex<()>>,
}
pub fn router(state: AppState) -> Router {
    let secured = Router::new()
        .route("/api/state", get(snapshot))
        .route("/api/command", post(command))
        .layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new()
        .merge(secured)
        .route(
            "/health",
            get(|| async {
                Json(json!({"application":"crypto-bros","version":env!("CARGO_PKG_VERSION")}))
            }),
        )
        .route("/wallet", get(wallet_page))
        .route("/wallet/pair", post(pair_wallet))
        .route("/live", get(crate::live_api::page))
        .route("/live-wallet.js", get(crate::live_api::script))
        .route("/live/preview", post(crate::live_api::preview))
        .route("/live/submit", post(crate::live_api::submit))
        .with_state(state)
}
async fn authorize(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.token);
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| value == expected);
    // Browser origins are not trusted merely because the listener is localhost.
    let origin_ok = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|origin| {
            matches!(
                origin,
                "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
            )
        });
    if !authorized || !origin_ok {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(axum::body::Body::from("Unauthorized"))
            .unwrap_or_default();
    }
    next.run(request).await
}
async fn snapshot(State(state): State<AppState>) -> Json<Value> {
    let e = state.engine.lock().await;
    let mut value = json!(&e.snapshot);
    value["wallet_readout"] = crate::readout::view(&e.snapshot, chrono::Utc::now().timestamp());
    value["real_advice"] = crate::live_api::advice(&e.snapshot);
    value["trade_watch"] = crate::watch::view(&e.snapshot, chrono::Utc::now().timestamp());
    Json(value)
}
#[derive(Deserialize)]
pub struct Command {
    pub action: String,
    #[serde(default)]
    pub payload: Value,
}
async fn command(
    State(state): State<AppState>,
    Json(cmd): Json<Command>,
) -> (StatusCode, Json<Value>) {
    match dispatch(&state, cmd).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))),
    }
}
pub async fn refresh(state: &AppState) -> Result<(), String> {
    let result = market::candles(&state.client).await;
    let mut e = state.engine.lock().await;
    if e.snapshot.updating {
        return Err("Engine is quiescing for an update".into());
    }
    match result {
        Ok((candles, price)) => {
            if e.snapshot.source == "synthetic_demo" && !e.snapshot.ledger.fills.is_empty() {
                return Err(
                    "Use a separate fresh paper account before mixing synthetic and market fills"
                        .into(),
                );
            }
            if e.snapshot.source != "binance_solusdc" {
                e.snapshot.running = false;
                e.snapshot.active_model = None;
                e.snapshot.research = None;
                e.snapshot.research_end = 0;
                e.snapshot.next_research_at = 0;
                e.snapshot.last_processed_candle = 0;
                e.snapshot.last_entry_candle = 0;
            }
            e.snapshot.candles = candles;
            e.snapshot.price = price;
            e.snapshot.updated_at = chrono::Utc::now().timestamp();
            e.snapshot.source = "binance_solusdc".into();
            e.snapshot.market_error = None;
            e.snapshot.evaluate()?;
            e.commit("market_refresh")
        }
        Err(error) => {
            e.snapshot.market_error = Some(error.clone());
            e.snapshot.running = false;
            e.commit("market_unavailable")?;
            Err(error)
        }
    }
}
pub async fn research(state: &AppState) -> Result<(), String> {
    let (candles, source, end) = {
        let mut e = state.engine.lock().await;
        if e.snapshot.optimizing || e.snapshot.updating {
            return Err("An optimization is already running".into());
        }
        let end = e.snapshot.candles.last().map_or(0, |c| c.time);
        let holdout_start = e
            .snapshot
            .candles
            .get(e.snapshot.candles.len() * 82 / 100 + 4)
            .map_or(0, |c| c.time);
        if e.snapshot.research_end > 0 && holdout_start <= e.snapshot.research_end {
            return Err("Wait for at least 180 fresh candles before another search; the previous holdout must not be reused for tuning".into());
        }
        e.snapshot.optimizing = true;
        (e.snapshot.candles.clone(), e.snapshot.source.clone(), end)
    };
    let result = tokio::task::spawn_blocking(move || optimize(&candles))
        .await
        .map_err(|_| "Research task did not complete");
    let mut e = state.engine.lock().await;
    e.snapshot.optimizing = false;
    if e.snapshot.updating || e.snapshot.source != source {
        return Err("Research discarded because the source changed or an update began".into());
    }
    let research = result??;
    e.snapshot.research_end = end;
    e.snapshot.next_research_at = chrono::Utc::now().timestamp() + 72 * 3600;
    e.snapshot.research = Some(research);
    e.commit("research_candidate_saved")
}
async fn dispatch(state: &AppState, cmd: Command) -> Result<Value, String> {
    let _live_operation = state.live_guard.lock().await;
    if state.engine.lock().await.snapshot.updating
        && !matches!(cmd.action.as_str(), "shutdown" | "pause")
    {
        return Err("Update in progress; restart the service to resume".into());
    }
    if cmd.action.starts_with("live_") || cmd.action == "enable_live" {
        return crate::live_api::dispatch(state, &cmd.action, &cmd.payload).await;
    }
    match cmd.action.as_str() {
        "refresh" => {
            refresh(state).await?;
            return Ok(json!({"ok":true}));
        }
        "research" => {
            research(state).await?;
            return Ok(json!({"ok":true}));
        }
        "quote" => {
            let key = std::env::var("CRYPTO_BROS_JUPITER_API_KEY").ok();
            let quote = market::quote(&state.client, 150 * USDC, key.as_deref()).await?;
            let mut e = state.engine.lock().await;
            e.snapshot.quote = Some(quote.clone());
            e.commit("jupiter_quote_read")?;
            return Ok(json!(quote));
        }
        "balance" => {
            let address = state
                .engine
                .lock()
                .await
                .snapshot
                .wallet
                .as_ref()
                .map(|w| w.address.clone())
                .ok_or("Connect a public wallet first")?;
            let wallet = market::wallet(&state.client, &address).await?;
            let mut e = state.engine.lock().await;
            if e.snapshot
                .wallet
                .as_ref()
                .is_none_or(|w| w.address != address)
            {
                return Err("The selected wallet changed during the balance request".into());
            }
            e.snapshot.wallet = Some(wallet);
            e.commit("public_balance_refreshed")?;
            return Ok(json!({"ok":true}));
        }
        _ => {}
    }
    let mut e = state.engine.lock().await;
    match cmd.action.as_str() {
        "archive_demo" => {
            if e.snapshot.source != "synthetic_demo"
                || e.snapshot.real.is_some()
                || e.snapshot.optimizing
            {
                return Err(
                    "Only an idle synthetic demo without a funded real account can be archived"
                        .into(),
                );
            }
            e.snapshot.running = false;
            e.commit("demo_paused_for_archive")?;
            e.store.backup(&state.data_dir.join(format!(
                "archived-demo-{}.sqlite",
                chrono::Utc::now().timestamp_millis()
            )))?;
            let mut fresh = Snapshot::new()?;
            fresh.wallet = e.snapshot.wallet.clone();
            fresh.email_configured = e.snapshot.email_configured;
            fresh.jupiter_key_configured = e.snapshot.jupiter_key_configured;
            e.snapshot = fresh;
        }
        "demo" => {
            if e.snapshot.real.is_some() {
                return Err(
                    "Synthetic prices cannot replace the feed of a funded real account".into(),
                );
            }
            if e.snapshot.optimizing {
                return Err("Wait for research to finish before switching sources".into());
            }
            if !e.snapshot.ledger.fills.is_empty() {
                return Err("Demo cannot replace a paper account that already has fills".into());
            }
            e.snapshot.running = false;
            e.snapshot.candles = demo_candles();
            e.snapshot.price = e.snapshot.candles.last().map_or(0.0, |c| c.close);
            e.snapshot.source = "synthetic_demo".into();
            e.snapshot.updated_at = chrono::Utc::now().timestamp();
            e.snapshot.research = None;
            e.snapshot.active_model = None;
            e.snapshot.research_end = 0;
            e.snapshot.next_research_at = 0;
            e.snapshot.last_processed_candle = 0;
            e.snapshot.last_entry_candle = 0;
            e.snapshot.policy = Default::default();
            e.snapshot.market_error = None;
            e.snapshot.revalue()?;
        }
        "demo_step" => {
            if e.snapshot.source != "synthetic_demo" {
                return Err("Replay stepping is only available for explicit synthetic data".into());
            }
            let last = e.snapshot.candles.last().ok_or("No replay candle")?.clone();
            let index = e.snapshot.candles.len() as f64;
            let price = (last.close + 0.4 * (index / 3.0).sin()).max(1.0);
            e.snapshot.candles.push(Candle {
                time: last.time + 900,
                open: last.close,
                high: last.close.max(price) + 0.3,
                low: last.close.min(price) - 0.3,
                close: price,
                volume: 1000.0,
            });
            e.snapshot.price = price;
            e.snapshot.updated_at = chrono::Utc::now().timestamp();
            e.snapshot.evaluate()?;
        }
        "activate_paper" => {
            let r = e.snapshot.research.clone().ok_or("Run research first")?;
            e.snapshot.active_model = Some(r.model);
            e.snapshot.policy = r.policy;
            e.snapshot.running = false;
            e.snapshot.decision("model","Research candidate selected for paper trading only. Live execution remains disabled.".into(),0.0,0.0);
        }
        "start" => {
            e.snapshot.revalue()?;
            if e.snapshot.risk_blocked() {
                return Err("Hard risk limit remains active; review the open positions".into());
            }
            if e.snapshot.active_model.is_none() {
                return Err("Train and select a paper model first".into());
            }
            if !e.snapshot.fresh() {
                return Err("Fresh market data is required".into());
            }
            if e.snapshot.alerts.iter().any(|a| !a.acknowledged) {
                return Err("Review outstanding risk alerts first".into());
            }
            e.snapshot.running = true;
            e.snapshot.evaluate()?;
        }
        "pause" => {
            e.snapshot.running = false;
            crate::live_api::pause(&mut e);
        }
        "buy" => {
            e.snapshot.paper_buy()?;
            e.snapshot.decision(
                "buy",
                "Manual paper layer; no real funds moved.".into(),
                0.0,
                0.0,
            );
        }
        "sell" => {
            if !e.snapshot.fresh() {
                return Err("Fresh prices are required to simulate a sale".into());
            }
            let id = cmd
                .payload
                .get("id")
                .and_then(Value::as_u64)
                .ok_or("Missing layer id")?;
            let percent = cmd
                .payload
                .get("percent")
                .and_then(Value::as_u64)
                .unwrap_or(100);
            if !(1..=100).contains(&percent) {
                return Err("Sale percent must be 1 to 100".into());
            }
            let layer = e
                .snapshot
                .ledger
                .layers
                .iter()
                .find(|l| l.id == id)
                .ok_or("Layer not found")?;
            let quantity = (layer.quantity as u128 * percent as u128 / 100) as u64;
            let price = price_units(
                e.snapshot.price * (1.0 - e.snapshot.policy.slippage_bps as f64 / 10_000.0),
            )
            .map_err(|e| e.to_string())?;
            let fee = e.snapshot.policy.fee_bps;
            e.snapshot
                .ledger
                .sell(id, quantity, price, fee, chrono::Utc::now().timestamp())
                .map_err(|e| e.to_string())?;
            e.snapshot.decision(
                "sell",
                format!("User approved paper sale of {percent}% of layer {id}."),
                0.0,
                0.0,
            );
            e.snapshot.revalue()?;
        }
        "reinvest" => {
            e.snapshot.ledger.parked_profit = 0;
            e.snapshot.decision(
                "allocate",
                "User released parked paper profit into available trading cash. Reserve unchanged."
                    .into(),
                0.0,
                0.0,
            );
        }
        "feedback" => {
            let id = cmd
                .payload
                .get("decision_id")
                .and_then(Value::as_u64)
                .ok_or("Missing decision id")?;
            let rating = cmd
                .payload
                .get("rating")
                .and_then(Value::as_str)
                .ok_or("Missing rating")?;
            if !["good", "too_early", "too_risky", "bad"].contains(&rating) {
                return Err("Unknown feedback rating".into());
            }
            if !e.snapshot.decisions.iter().any(|d| d.id == id) {
                return Err("Decision not found".into());
            }
            let note = cmd
                .payload
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(500)
                .collect();
            e.snapshot.feedback.push(Feedback {
                decision_id: id,
                rating: rating.into(),
                note,
                time: chrono::Utc::now().timestamp(),
            });
        }
        "acknowledge" => {
            for a in &mut e.snapshot.alerts {
                a.acknowledged = true;
            }
        }
        "wallet_address" => {
            let address = cmd
                .payload
                .get("address")
                .and_then(Value::as_str)
                .ok_or("Public address required")?;
            if !market::valid_address(address) {
                return Err("Invalid Solana public address".into());
            }
            if e.snapshot
                .real
                .as_ref()
                .is_some_and(|a| a.address != address)
            {
                return Err("This data folder is bound to its funded bot wallet; use a separate account folder for another wallet".into());
            }
            e.snapshot.wallet = Some(Wallet {
                address: address.into(),
                sol: 0.0,
                usdc: 0.0,
                verified_at: None,
            });
        }
        "wallet_link" => {
            let ticket = uuid::Uuid::new_v4().to_string();
            e.pair_ticket = Some((ticket.clone(), chrono::Utc::now().timestamp() + 300));
            return Ok(json!({"url":format!("http://127.0.0.1:{}/wallet#{}",state.port,ticket)}));
        }
        "prepare_update" => {
            if e.snapshot
                .real
                .as_ref()
                .and_then(|a| a.pending.as_ref())
                .is_some_and(|p| p.signature.is_some())
            {
                return Err(
                    "A signed transaction is still unresolved; reconcile it before updating".into(),
                );
            }
            e.snapshot.running = false;
            e.snapshot.watch.enabled = false;
            crate::live_api::pause(&mut e);
            e.snapshot.updating = true;
            e.commit("paused_for_update")?;
            let backup = state.data_dir.join(format!(
                "backup-{}.sqlite",
                chrono::Utc::now().timestamp_millis()
            ));
            e.store.backup(&backup)?;
            return Ok(json!({"ok":true,"pending_transactions":0,"backup_created":true}));
        }
        "shutdown" => {
            e.snapshot.running = false;
            e.snapshot.watch.enabled = false;
            crate::live_api::pause(&mut e);
            e.commit("service_shutdown")?;
            state.shutdown.notify_one();
            return Ok(json!({"ok":true}));
        }
        _ => return Err("Unknown command".into()),
    }
    e.commit(&cmd.action)?;
    Ok(json!({"ok":true}))
}

async fn wallet_page() -> (HeaderMap, Html<&'static str>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    (headers, Html(include_str!("wallet.html")))
}
#[derive(Deserialize)]
struct PairRequest {
    ticket: String,
    address: String,
}
async fn pair_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PairRequest>,
) -> (StatusCode, Json<Value>) {
    let origin = format!("http://127.0.0.1:{}", state.port);
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(origin.as_str()) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"Origin rejected"})),
        );
    }
    let _operation = state.live_guard.lock().await;
    let mut e = state.engine.lock().await;
    if e.snapshot.updating {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Update in progress"})),
        );
    }
    if !e.pair_ticket.as_ref().is_some_and(|(ticket, expiry)| {
        *ticket == request.ticket && *expiry >= chrono::Utc::now().timestamp()
    }) || !market::valid_address(&request.address)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"Pairing link is invalid or expired"})),
        );
    }
    if e.snapshot
        .real
        .as_ref()
        .is_some_and(|a| a.address != request.address)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"This funded account is bound to its dedicated bot wallet"})),
        );
    }
    e.pair_ticket = None;
    e.snapshot.wallet = Some(Wallet {
        address: request.address,
        sol: 0.0,
        usdc: 0.0,
        verified_at: None,
    });
    match e.commit("phantom_public_address_connected") {
        Ok(()) => (StatusCode::OK, Json(json!({"ok":true}))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":error})),
        ),
    }
}

pub async fn background(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.tick().await;
    loop {
        interval.tick().await;
        let (live_source, should_research) = {
            let e = state.engine.lock().await;
            (
                e.snapshot.source == "binance_solusdc",
                e.snapshot.next_research_at > 0
                    && chrono::Utc::now().timestamp() >= e.snapshot.next_research_at,
            )
        };
        if live_source {
            let _ = refresh(&state).await;
        }
        {
            let _operation = state.live_guard.lock().await;
            let has_real = state.engine.lock().await.snapshot.real.is_some();
            if has_real {
                let _ = crate::live_api::reconcile(&state).await;
                let address = state
                    .engine
                    .lock()
                    .await
                    .snapshot
                    .real
                    .as_ref()
                    .map(|a| a.address.clone());
                if let Some(address) = address {
                    match crate::chain::balances(&state.client, &address).await {
                        Ok(balances) => {
                            let mut e = state.engine.lock().await;
                            e.snapshot.real_balances = Some(balances);
                            e.snapshot.watch.error = None;
                        }
                        Err(_) => {
                            state.engine.lock().await.snapshot.watch.error = Some(
                                "ตรวจยอดกระเป๋าไม่ได้ชั่วคราว ระบบจะลองใหม่และยังไม่เสนอรายการใหม่".into(),
                            )
                        }
                    }
                }
                if let Ok(rate) = crate::chain::rate(&state.client).await {
                    let mut e = state.engine.lock().await;
                    e.snapshot.thb_rate = Some(rate);
                    let advice = crate::live_api::advice(&e.snapshot);
                    let candle = e.snapshot.candles.last().map_or(0, |c| c.time);
                    if e.snapshot
                        .real
                        .as_ref()
                        .is_some_and(|a| a.last_advice_candle < candle)
                        && e.snapshot.fresh()
                    {
                        if let Some(a) = e.snapshot.real.as_mut() {
                            a.last_advice_candle = candle;
                        }
                        let action = format!(
                            "live_advice_{}",
                            advice["action"].as_str().unwrap_or("wait")
                        );
                        e.snapshot.decision(
                            &action,
                            advice["reason"]
                                .as_str()
                                .unwrap_or("No live advice available")
                                .into(),
                            advice["forecast"].as_f64().unwrap_or(0.0),
                            advice["net_edge"].as_f64().unwrap_or(0.0),
                        );
                        let version = e
                            .snapshot
                            .real
                            .as_ref()
                            .and_then(|a| a.advice_model.as_ref())
                            .map_or("manual-live".into(), |m| m.version.clone());
                        if let Some(d) = e.snapshot.decisions.last_mut() {
                            d.model = version;
                        }
                    }
                    if advice["risk_blocked"] == true {
                        e.snapshot.live_enabled = false;
                        e.snapshot.alert("REAL account risk review: a layer review price, 10% bot-capital loss or 50% allocated historical THB-cost loss has been reached. New buys are paused. No automatic stop-loss sale was executed; review and approve any sale in Phantom.".into());
                    }
                    let _ = e.commit("real_valuation_refreshed");
                }
            }
        }
        if should_research {
            let _ = research(&state).await;
        }
        let email_job = {
            let e = state.engine.lock().await;
            e.snapshot.alerts.iter().find(|a|!a.acknowledged && a.attempts<3 && (a.email_status=="queued" || a.email_status=="retry")).map(|a|(a.id,format!("{}\nOpen Crypto Bros to review the relevant real or paper account. Real sales require your Phantom approval; risk alerts do not execute stop-loss orders.",a.message)))
        };
        if let Some((id, message)) = email_job {
            let result = market::email(&state.client, &message).await;
            let mut e = state.engine.lock().await;
            if let Some(alert) = e.snapshot.alerts.iter_mut().find(|a| a.id == id) {
                alert.attempts += 1;
                alert.email_status = if result.is_ok() {
                    "accepted_by_provider"
                } else if alert.attempts >= 3 {
                    "failed"
                } else {
                    "retry"
                }
                .into();
            }
            let _ = e.commit("email_delivery_status");
        }
    }
}
