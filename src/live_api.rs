use crate::{
    api::{AppState, Engine},
    chain,
    live::{self, LiveConfig, RealAccount},
    solana_wire,
    state::Snapshot,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Html,
};
use crypto_bros_core::ledger::price_units;
use serde::Deserialize;
use serde_json::{Value, json};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn units(payload: &Value, name: &str, decimals: u32) -> Result<u64, String> {
    live::decimal_units(
        payload[name]
            .as_str()
            .ok_or_else(|| format!("Enter {name}"))?,
        decimals,
    )
}
pub fn pause(engine: &mut Engine) {
    engine.snapshot.live_enabled = false;
    engine.live_ticket = None;
    if let Some(account) = engine.snapshot.real.as_mut() {
        if account
            .pending
            .as_ref()
            .is_some_and(|p| p.signature.is_none())
        {
            account.pending = None;
        }
    }
}
fn market_ready(s: &Snapshot) -> Result<u64, String> {
    if s.source != "binance_solusdc" || !s.fresh() || s.market_error.is_some() || s.updating {
        return Err("Refresh real SOL/USDC market prices before preparing an order".into());
    }
    price_units(s.price).map_err(|e| e.to_string())
}
pub fn advice(s: &Snapshot) -> Value {
    let Some(account) = s.real.as_ref() else {
        return json!({"action":"setup","reason":"Set up a dedicated funded wallet"});
    };
    let Ok(price) = market_ready(s) else {
        return json!({"action":"wait","reason":"Real market data is stale or unavailable"});
    };
    let Some(rate) = s.thb_rate.as_ref().filter(|r| r.fresh(now())) else {
        return json!({"action":"wait","reason":"Refresh the USDC/THB reference rate"});
    };
    let equity = account.equity(price).unwrap_or(0);
    let value_thb = rate.satang(equity).unwrap_or(0);
    let net_thb = value_thb as i128 - account.fees_satang as i128;
    let drawdown =
        account.entry_blocked(price) || net_thb * 2 <= account.initial_cost_satang as i128;
    let exit = account
        .layers
        .iter()
        .find(|l| l.quantity > 0 && (price >= l.target_price || price <= l.review_price));
    let (prediction, edge) = match (&account.advice_model, &s.indicators) {
        (Some(m), Some(i)) => {
            let p = m.predict(i.features);
            (p, account.advice_policy.net_edge(p, m.uncertainty))
        }
        _ => (0.0, 0.0),
    };
    let (action, reason) = if account.pending.is_some() {
        ("pending", "Resolve the current order first")
    } else if drawdown {
        (
            "review",
            "Risk threshold reached. New buys paused; review a sale yourself",
        )
    } else if exit.is_some() {
        (
            "sell",
            "A layer reached its target or review price; a sale still needs your signature",
        )
    } else if !s.live_enabled {
        ("paused", "New buy proposals are paused")
    } else if account.advice_model.is_none() {
        (
            "wait",
            "Select a research candidate explicitly for live advice",
        )
    } else if edge > account.advice_policy.min_edge {
        (
            "buy",
            "Model forecast clears its uncertainty and estimated trading costs",
        )
    } else {
        (
            "wait",
            "Model forecast does not clear uncertainty and estimated costs",
        )
    };
    json!({"action":action,"reason":reason,"forecast":prediction,"net_edge":edge,"layer_id":exit.map(|l|l.id),
        "equity_usdc":equity,"equity_satang":value_thb,"bot_pnl_usdc":equity as i128-account.initial_usdc as i128-account.fees_usdc as i128,
        "historical_pnl_satang":net_thb-account.initial_cost_satang as i128,
        "bot_pnl_satang":net_thb-account.initial_reference_satang as i128,"risk_blocked":drawdown})
}
pub async fn dispatch(state: &AppState, action: &str, payload: &Value) -> Result<Value, String> {
    match action {
        "live_refresh" => {
            crate::api::refresh(state).await?;
            let rate = chain::rate(&state.client).await?;
            let address = state
                .engine
                .lock()
                .await
                .snapshot
                .wallet
                .as_ref()
                .map(|w| w.address.clone())
                .ok_or("Connect the bot wallet first")?;
            let balances = chain::balances(&state.client, &address).await?;
            let mut e = state.engine.lock().await;
            e.snapshot.thb_rate = Some(rate);
            e.snapshot.real_balances = Some(balances);
            e.commit("real_balances_and_thb_refreshed")?;
        }
        "live_setup" => {
            let address = {
                let e = state.engine.lock().await;
                if e.snapshot.real.is_some() {
                    return Err("A funded account is already configured; its cost history cannot be overwritten".into());
                }
                market_ready(&e.snapshot)?;
                e.snapshot
                    .wallet
                    .as_ref()
                    .map(|w| w.address.clone())
                    .ok_or("Connect the dedicated bot wallet first")?
            };
            let rate = chain::rate(&state.client).await?;
            let balances = chain::balances(&state.client, &address).await?;
            let funded = units(payload, "received_usdc", 6)?;
            let config = LiveConfig {
                trading_budget: units(payload, "budget_usdc", 6)?,
                max_buy: units(payload, "max_buy_usdc", 6)?,
                reserve: units(payload, "reserve_usdc", 6)?,
                max_layers: 2,
                sol_gas_reserve: 5_000_000,
                max_network_fee: 1_000_000,
                slippage_bps: 50,
            };
            if balances.usdc < funded || balances.sol < config.sol_gas_reserve {
                return Err("The dedicated wallet needs the allocated USDC plus at least 0.005 SOL reserved for network costs; this is a buffer, not a fee quote".into());
            }
            let account = RealAccount::new(
                address,
                funded,
                units(payload, "source_usdc", 6)?,
                units(payload, "source_thb", 2)?,
                payload["fees_included"] == true,
                config,
                &rate,
                now(),
            )?;
            let mut e = state.engine.lock().await;
            e.snapshot.real = Some(account);
            e.snapshot.thb_rate = Some(rate);
            e.snapshot.real_balances = Some(balances);
            e.snapshot.live_enabled = false;
            e.commit("real_account_funding_recorded")?;
        }
        "enable_live" => {
            let mut e = state.engine.lock().await;
            market_ready(&e.snapshot)?;
            let a = e
                .snapshot
                .real
                .as_ref()
                .ok_or("Set up the funded bot wallet and limits first")?;
            if a.pending.is_some() || a.reconciliation_error.is_some() {
                return Err("Reconcile the pending transaction before enabling new entries".into());
            }
            e.snapshot.live_enabled = true;
            e.commit("manual_signing_mode_enabled")?;
        }
        "live_pause" => {
            let mut e = state.engine.lock().await;
            pause(&mut e);
            e.commit("live_entries_paused")?;
        }
        "live_use_model" => {
            let mut e = state.engine.lock().await;
            market_ready(&e.snapshot)?;
            let candidate = e
                .snapshot
                .research
                .clone()
                .ok_or("Run research on real market candles first")?;
            let a = e
                .snapshot
                .real
                .as_mut()
                .ok_or("Set up the bot wallet first")?;
            if a.pending.is_some() {
                return Err("Resolve the pending order before changing the advice model".into());
            }
            a.advice_model = Some(candidate.model);
            a.advice_policy = candidate.policy;
            e.commit("live_advice_model_selected_by_user")?;
        }
        "live_reinvest" => {
            let mut e = state.engine.lock().await;
            let a = e
                .snapshot
                .real
                .as_mut()
                .ok_or("Set up the bot wallet first")?;
            if a.pending.is_some() {
                return Err("Resolve the pending order first".into());
            }
            a.parked_profit = 0;
            e.commit("live_profit_released_with_existing_caps")?;
        }
        "live_cancel" => {
            let mut e = state.engine.lock().await;
            let a = e.snapshot.real.as_mut().ok_or("No live account")?;
            if a.pending.as_ref().is_some_and(|p| p.signature.is_some()) {
                return Err(
                    "A submitted transaction cannot be cancelled; reconcile its signature".into(),
                );
            }
            a.pending = None;
            e.live_ticket = None;
            e.commit("unsigned_live_order_cancelled")?;
        }
        "live_prepare" => {
            let side = payload["side"].as_str().ok_or("Choose buy or sell")?;
            if !["buy", "sell"].contains(&side) {
                return Err("Choose buy or sell".into());
            }
            let rate = chain::rate(&state.client).await?;
            let (account, input, layer_id, price, target, review, model) = {
                let mut e = state.engine.lock().await;
                e.snapshot.thb_rate = Some(rate.clone());
                let price = market_ready(&e.snapshot)?;
                let a = e
                    .snapshot
                    .real
                    .as_ref()
                    .ok_or("Set up the funded bot wallet first")?;
                if a.pending.is_some() || a.reconciliation_error.is_some() {
                    return Err("Resolve the previous order or reconciliation issue first".into());
                }
                let manual = payload["manual"] == true;
                let (input, id) = if side == "buy" {
                    if !e.snapshot.live_enabled {
                        return Err("Enable manually approved buy proposals first".into());
                    }
                    let input = units(payload, "amount_usdc", 6)?;
                    a.check_buy(input, price)?;
                    let advice = advice(&e.snapshot);
                    if advice["risk_blocked"] == true {
                        return Err("A capital-loss review blocks new buys".into());
                    }
                    if !manual && advice["action"] != "buy" {
                        return Err(format!(
                            "Model recommends waiting: {}",
                            advice["reason"].as_str().unwrap_or("No entry signal")
                        ));
                    }
                    if !manual
                        && a.fills
                            .iter()
                            .filter(|f| f.side == "buy")
                            .any(|f| now() - f.time < a.advice_policy.cooldown_bars as i64 * 900)
                    {
                        return Err("The model's layer cooldown is still active".into());
                    }
                    (input, 0)
                } else {
                    let id = payload["layer_id"].as_u64().ok_or("Choose a real layer")?;
                    let layer = a
                        .layers
                        .iter()
                        .find(|l| l.id == id && l.quantity > 0)
                        .ok_or("Open real layer not found")?;
                    let percent = payload["percent"].as_u64().unwrap_or(100);
                    if !(1..=100).contains(&percent) {
                        return Err("Sale percent must be between 1 and 100".into());
                    }
                    (live::ratio(layer.quantity, percent, 100)?, id)
                };
                let atr = e
                    .snapshot
                    .indicators
                    .as_ref()
                    .map_or(e.snapshot.price * 0.02, |i| i.atr);
                let target = price_units(e.snapshot.price + a.advice_policy.take_profit_atr * atr)
                    .map_err(|e| e.to_string())?;
                let review =
                    price_units((e.snapshot.price - a.advice_policy.review_atr * atr).max(0.01))
                        .map_err(|e| e.to_string())?;
                let model = if manual {
                    "manual-live".into()
                } else {
                    a.advice_model
                        .as_ref()
                        .map_or("manual-exit".into(), |m| m.version.clone())
                };
                (a.clone(), input, id, price, target, review, model)
            };
            let order = chain::prepare(
                &state.client,
                &account,
                side,
                input,
                layer_id,
                price,
                rate,
                target,
                review,
                model,
            )
            .await?;
            let mut e = state.engine.lock().await;
            if side == "buy" && payload["manual"] != true {
                let signal = advice(&e.snapshot);
                let existing_cost = 2.0
                    * (account.advice_policy.fee_bps + account.advice_policy.slippage_bps) as f64
                    / 10_000.0;
                let quoted_value = order.minimum_output as f64 * price as f64 / live::SOL as f64;
                let network_cost = order.network_fee as f64 * price as f64 / live::SOL as f64;
                let projected_cost = 2.0
                    * ((1.0 - quoted_value / input as f64).max(0.0) + network_cost / input as f64);
                let adjusted_edge = signal["net_edge"]
                    .as_f64()
                    .ok_or("Model advice is no longer available")?
                    + existing_cost
                    - projected_cost;
                if adjusted_edge <= account.advice_policy.min_edge {
                    return Err("The real quote, slippage allowance and estimated round-trip network costs consume the model's predicted edge".into());
                }
            }
            e.snapshot
                .real
                .as_mut()
                .ok_or("Live account missing")?
                .pending = Some(order);
            e.commit("unsigned_live_order_prepared")?;
        }
        "live_link" => {
            let mut e = state.engine.lock().await;
            let order = e
                .snapshot
                .real
                .as_ref()
                .and_then(|a| a.pending.as_ref())
                .ok_or("Prepare an order first")?;
            if order.signature.is_some() || order.expires_at <= now() {
                return Err(
                    "Order already submitted or expired; reconcile or prepare again".into(),
                );
            }
            let ticket = uuid::Uuid::new_v4().to_string();
            let id = order.id.clone();
            let expiry = order.expires_at;
            e.live_ticket = Some((ticket.clone(), id, expiry));
            return Ok(json!({"url":format!("http://127.0.0.1:{}/live#{}",state.port,ticket)}));
        }
        "live_reconcile" => {
            reconcile(state).await?;
        }
        _ => return Err("Unknown live command".into()),
    }
    Ok(json!({"ok":true}))
}

pub async fn reconcile(state: &AppState) -> Result<(), String> {
    let (address, order) = {
        let e = state.engine.lock().await;
        let Some(a) = &e.snapshot.real else {
            return Ok(());
        };
        let Some(p) = a.pending.as_ref().filter(|p| p.signature.is_some()) else {
            return Ok(());
        };
        (a.address.clone(), p.clone())
    };
    let result = chain::settlement(&state.client, &address, &order).await;
    let mut e = state.engine.lock().await;
    let a = e.snapshot.real.as_mut().ok_or("Live account missing")?;
    if a.pending.as_ref().is_none_or(|p| p.id != order.id) {
        return Err("Pending order changed during reconciliation".into());
    }
    let mut report = None;
    match result {
        Ok(chain::Settlement::Pending) => {}
        Ok(chain::Settlement::Expired) => {
            a.pending = None;
            report = Some(
                "Signed order expired without a recorded transaction; no swap booked".to_string(),
            );
        }
        Ok(chain::Settlement::Filled {
            input,
            output,
            fee,
            rent,
            time,
        }) => {
            let signature = order.signature.as_deref().ok_or("Missing signed order")?;
            match a.apply_fill(&order, signature, input, output, fee, rent, time) {
                Ok(()) => {
                    a.reconciliation_error = None;
                    report = Some(
                        "Finalized transaction reconciled using actual chain balances and fees"
                            .into(),
                    );
                }
                Err(error) => {
                    a.reconciliation_error = Some(error.clone());
                    e.snapshot.live_enabled = false;
                    e.snapshot.alert(error);
                }
            }
        }
        Ok(chain::Settlement::Failed { fee, time }) => {
            let fee_usdc = live::ratio(fee, order.price, live::SOL)?;
            let fee_thb = order.rate.satang(fee_usdc)?;
            let total_usdc = a.fees_usdc.checked_add(fee_usdc).ok_or("Fee overflow")?;
            let total_thb = a.fees_satang.checked_add(fee_thb).ok_or("Fee overflow")?;
            a.fees_usdc = total_usdc;
            a.fees_satang = total_thb;
            a.fills.push(live::RealFill {
                signature: order.signature.clone().ok_or("Missing signature")?,
                layer_id: order.layer_id,
                side: "failed".into(),
                input: 0,
                output: 0,
                network_fee_lamports: fee,
                rent_delta_lamports: 0,
                realized_usdc: 0,
                realized_satang: 0,
                rate: order.rate.clone(),
                time,
            });
            a.pending = None;
            report = Some(
                "Transaction failed on chain. No swap booked; the actual network fee was recorded"
                    .into(),
            );
        }
        Err(error) => {
            if let Some(p) = a.pending.as_mut() {
                p.message = Some(error.clone());
            }
            e.snapshot.live_enabled = false;
            e.commit("live_reconciliation_waiting")?;
            return Err(error);
        }
    }
    if let Some(message) = report {
        e.live_ticket = None;
        e.snapshot.decision("live_settlement", message, 0.0, 0.0);
    }
    e.commit("live_transaction_reconciled")
}

fn page_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    headers
}
pub async fn page() -> (HeaderMap, Html<&'static str>) {
    (page_headers(), Html(include_str!("live-wallet.html")))
}
pub async fn script() -> (HeaderMap, &'static str) {
    let mut h = page_headers();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript"),
    );
    (h, include_str!(concat!(env!("OUT_DIR"), "/live-wallet.js")))
}
#[derive(Deserialize)]
pub struct TicketRequest {
    ticket: String,
    #[serde(default)]
    signed_transaction: String,
}
fn ticket_valid(e: &Engine, request: &TicketRequest) -> Result<(), String> {
    let order = e
        .snapshot
        .real
        .as_ref()
        .and_then(|a| a.pending.as_ref())
        .ok_or("This order is no longer pending")?;
    if !e.live_ticket.as_ref().is_some_and(|(ticket, id, expiry)| {
        ticket == &request.ticket && id == &order.id && *expiry > now()
    }) || order.signature.is_some()
        || order.expires_at <= now()
        || e.snapshot.updating
        || (order.side == "buy" && !e.snapshot.live_enabled)
    {
        return Err("This approval link expired, was cancelled or has already been used".into());
    }
    Ok(())
}
fn origin_valid(state: &AppState, headers: &HeaderMap) -> bool {
    headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
        == Some(format!("http://127.0.0.1:{}", state.port).as_str())
}
pub async fn preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TicketRequest>,
) -> (StatusCode, Json<Value>) {
    if !origin_valid(&state, &headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"Origin rejected"})),
        );
    }
    let e = state.engine.lock().await;
    match ticket_valid(&e, &request) {
        Ok(()) => {
            let a = e.snapshot.real.as_ref();
            (
                StatusCode::OK,
                Json(
                    json!({"address":a.map(|a|&a.address),"order":a.and_then(|a|a.pending.as_ref())}),
                ),
            )
        }
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))),
    }
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TicketRequest>,
) -> (StatusCode, Json<Value>) {
    if !origin_valid(&state, &headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"Origin rejected"})),
        );
    }
    let _operation = state.live_guard.lock().await;
    match submit_inner(&state, &request).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))),
    }
}
async fn submit_inner(state: &AppState, request: &TicketRequest) -> Result<Value, String> {
    let (account, order, signature) = {
        let e = state.engine.lock().await;
        ticket_valid(&e, request)?;
        let a = e.snapshot.real.as_ref().ok_or("Live account missing")?;
        let p = a.pending.as_ref().ok_or("Order missing")?;
        market_ready(&e.snapshot)?;
        if p.side == "buy" && advice(&e.snapshot)["risk_blocked"] == true {
            return Err("Capital-loss review now blocks this buy".into());
        }
        let signature = solana_wire::verify_signed(
            &p.unsigned_transaction,
            &request.signed_transaction,
            &a.address,
        )?;
        (a.clone(), p.clone(), signature)
    };
    chain::inspect(&state.client, &account, &order).await?;
    {
        let mut e = state.engine.lock().await;
        ticket_valid(&e, request)?;
        let p = e
            .snapshot
            .real
            .as_mut()
            .and_then(|a| a.pending.as_mut())
            .ok_or("Order missing")?;
        p.signature = Some(signature.clone());
        p.status = "submitted_pending_confirmation".into();
        e.live_ticket = None;
        // Durable intent BEFORE sending: timeout, crash and restart cannot authorize a second order.
        e.commit("signed_order_saved_before_submission")?;
    }
    let sent = chain::execute(
        &state.client,
        &order.request_id,
        &request.signed_transaction,
    )
    .await;
    if let Err(error) = sent {
        let mut e = state.engine.lock().await;
        if let Some(p) = e.snapshot.real.as_mut().and_then(|a| a.pending.as_mut()) {
            p.message = Some(error);
        }
        e.commit("submission_requires_reconciliation")?;
    }
    let _ = reconcile(state).await;
    Ok(
        json!({"ok":true,"signature":signature,"message":"Signature recorded. Check the app for finalized chain confirmation; this is not yet a confirmed fill."}),
    )
}
