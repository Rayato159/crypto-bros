use crypto_bros::{
    chain::Balances,
    live::{LiveConfig, RealAccount, RealLayer, SOL, ThbRate, USDC},
    state::Snapshot,
    storage::Store,
    watch,
};
use crypto_bros_core::{
    model::ForecastModel,
    research::{Metrics, Research, demo_candles},
};

// Injected gate evidence for deterministic tests; these are not backtest results.
fn candidate(model: ForecastModel) -> Research {
    Research {
        execution_version: 2,
        version: model.version.clone(),
        trained_through: model.training_end,
        model,
        policy: Default::default(),
        trials: 64,
        validation: Metrics::default(),
        holdout: Metrics {
            trades: 8,
            entries: 8,
            net_return: 0.01,
            max_drawdown: 0.02,
            ..Default::default()
        },
        baseline: Metrics::default(),
        status: "shadow_candidate".into(),
        holdout_start: 1,
        explanation: "Synthetic gate-test evidence, not measured performance".into(),
    }
}

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
    a.select_advice(&candidate(ForecastModel {
        version: "isolated-fixture".into(),
        weights: [0.05, 0.0, 0.0, 0.0, 0.0, 0.0],
        means: [0.0; 5],
        scales: [1.0; 5],
        uncertainty: 0.001,
        trained_samples: 200,
        training_end: 0,
    }))
    .unwrap();
    a.reserve_bps = 2000;
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
        reserved_quantity: 0,
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
    assert_eq!(p["quantity_sol_atomic"], SOL / 10);
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
        l.quantity,
        10_050_000,
        1_000_000,
        100 * USDC,
        s.thb_rate.as_ref().unwrap(),
    ));
    assert!(watch::profit_exit_covers_costs(
        l,
        l.quantity,
        11_000_000,
        1_000_000,
        100 * USDC,
        s.thb_rate.as_ref().unwrap(),
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

#[test]
fn first_watch_blocks_failed_or_unknown_candidates_but_keeps_monitoring() {
    for status in ["rejected", "insufficient_evidence", "unknown", ""] {
        let mut s = fixture();
        let a = s.real.as_mut().unwrap();
        let mut research = a.advice_research.take().unwrap();
        research.status = status.into();
        a.advice_model = None;
        s.research = Some(research);
        crypto_bros::live_api::start_watch(&mut s).unwrap();
        assert!(s.watch.enabled);
        assert!(!s.live_enabled);
        assert!(s.real.as_ref().unwrap().advice_model.is_none());
        assert!(cards(&s).is_empty());
        assert!(watch::claim(&mut s, chrono::Utc::now().timestamp()).is_none());
        assert_eq!(crypto_bros::live_api::advice(&s)["model_eligible"], false);
        assert_ne!(
            watch::view(&s, s.updated_at)["message"],
            "เฝ้าตลาดอยู่ ยังไม่มีรายการที่ผ่านเกณฑ์ให้กด"
        );
    }
}

#[test]
fn watch_uses_selected_sizes_and_risk_reviews_offer_only_the_unreserved_quantity() {
    let mut s = fixture();
    let mut c = s.real.as_ref().unwrap().advice_research.clone().unwrap();
    c.policy.buy_size_bps = 5000;
    c.policy.sell_size_bps = 5000;
    s.real.as_mut().unwrap().select_advice(&c).unwrap();
    assert_eq!(cards(&s)[0]["amount_usdc"], 15 * USDC);
    layer(&mut s, 1);
    s.real.as_mut().unwrap().layers[0].reserved_quantity = SOL / 50;
    s.price = 112.0;
    let signal = cards(&s)[0].clone();
    assert_eq!(signal["quantity_sol"], SOL * 4 / 100);
    assert_eq!(signal["pnl_usdc"], 476_000);
    let payload = watch::order_payload(&s, signal["key"].as_str().unwrap(), s.updated_at).unwrap();
    assert_eq!(payload["quantity_sol_atomic"], SOL * 4 / 100);
    s.price = 89.0;
    assert_eq!(cards(&s)[0]["quantity_sol"], SOL * 8 / 100);
    assert_eq!(cards(&s)[0]["risk"], true);
}

#[test]
fn first_watch_selects_only_a_passing_candidate_and_no_candidate_stays_blocked() {
    let mut s = fixture();
    let a = s.real.as_mut().unwrap();
    s.research = a.advice_research.take();
    a.advice_model = None;
    let mut empty = s.clone();
    empty.research = None;
    crypto_bros::live_api::start_watch(&mut empty).unwrap();
    assert!(!empty.live_enabled);
    assert!(cards(&empty).is_empty());
    crypto_bros::live_api::start_watch(&mut s).unwrap();
    assert!(s.live_enabled);
    assert_eq!(cards(&s)[0]["side"], "buy");
    assert!(s.real.as_ref().unwrap().advice_research.is_some());
}

#[tokio::test]
async fn candidate_selection_rejects_bad_results_without_changing_current_model_or_limits() {
    let dir = tempfile::tempdir().unwrap();
    let state = service(dir.path());
    let original = serde_json::to_value(&state.engine.lock().await.snapshot.real).unwrap();
    for status in ["rejected", "insufficient_evidence", "unexpected"] {
        {
            let mut e = state.engine.lock().await;
            let mut c = e
                .snapshot
                .real
                .as_ref()
                .unwrap()
                .advice_research
                .clone()
                .unwrap();
            c.status = status.into();
            e.snapshot.research = Some(c);
        }
        let result =
            crypto_bros::live_api::dispatch(&state, "live_use_model", &serde_json::json!({})).await;
        assert!(result.is_err(), "{status}");
        let e = state.engine.lock().await;
        assert_eq!(serde_json::to_value(&e.snapshot.real).unwrap(), original);
    }
    {
        let mut e = state.engine.lock().await;
        let mut c = e
            .snapshot
            .real
            .as_ref()
            .unwrap()
            .advice_research
            .clone()
            .unwrap();
        c.version = "passing-replacement".into();
        c.model.version = c.version.clone();
        e.snapshot.research = Some(c);
    }
    crypto_bros::live_api::dispatch(&state, "live_use_model", &serde_json::json!({}))
        .await
        .unwrap();
    let e = state.engine.lock().await;
    let updated = serde_json::to_value(&e.snapshot.real).unwrap();
    assert_eq!(updated["advice_model"]["version"], "passing-replacement");
    for field in ["config", "cash", "layers", "fills", "parked_profit"] {
        assert_eq!(updated[field], original[field]);
    }
}

#[test]
fn status_alone_cannot_approve_insufficient_invalid_or_mismatched_evidence() {
    let s = fixture();
    let original = s.real.as_ref().unwrap().advice_research.as_ref().unwrap();
    assert!(original.live_eligibility().is_ok());
    for field in 0..15 {
        let mut c = original.clone();
        match field {
            0 => c.holdout.trades = 7,
            1 => c.holdout.net_return = 0.0,
            2 => c.holdout.net_return = f64::NAN,
            3 => c.holdout.max_drawdown = 0.1,
            4 => c.holdout.max_drawdown = -0.01,
            5 => c.holdout.max_drawdown = f64::INFINITY,
            6 => c.version = "different".into(),
            7 => c.trained_through += 1,
            8 => c.holdout.net_return = -0.01,
            9 => c.holdout.entries = 1,
            10 => c.execution_version = 0,
            11 => c.model.weights[0] = f64::NAN,
            12 => c.model.scales[0] = 0.0,
            13 => c.policy.buy_size_bps = 0,
            _ => c.policy.sell_size_bps = 10001,
        }
        assert!(c.live_eligibility().is_err(), "field {field}");
    }
}

#[test]
fn selected_evidence_survives_restart_and_is_not_replaced_by_later_research() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("evidence.sqlite")).unwrap();
    let mut s = fixture();
    let mut next = s.real.as_ref().unwrap().advice_research.clone().unwrap();
    next.status = "rejected".into();
    next.version = "later-research".into();
    next.model.version = next.version.clone();
    s.research = Some(next);
    store.save(&s, "fixture").unwrap();
    let mut loaded = store.load().unwrap();
    assert!(!loaded.live_enabled);
    crypto_bros::live_api::start_watch(&mut loaded).unwrap();
    assert_eq!(cards(&loaded)[0]["side"], "buy");
    assert_eq!(
        loaded
            .real
            .as_ref()
            .unwrap()
            .advice_model
            .as_ref()
            .unwrap()
            .version,
        "isolated-fixture"
    );
}

#[test]
fn legacy_and_mismatched_models_never_propose_buys_but_preserve_both_exit_types() {
    for defect in 0..5 {
        let mut s = fixture();
        layer(&mut s, 1);
        if defect == 0 {
            let mut old = serde_json::to_value(s).unwrap();
            old["real"]
                .as_object_mut()
                .unwrap()
                .remove("advice_research");
            s = serde_json::from_value(old).unwrap();
        } else {
            let a = s.real.as_mut().unwrap();
            match defect {
                1 => a.advice_model.as_mut().unwrap().weights[0] += 0.01,
                2 => a.advice_policy.min_edge *= 0.5,
                3 => a.advice_research.as_mut().unwrap().status = "rejected".into(),
                _ => a.advice_research.as_mut().unwrap().status = "insufficient_evidence".into(),
            }
        }
        // A passing latest candidate must not silently certify a legacy/mismatched model.
        s.research = fixture().real.unwrap().advice_research;
        let cash = s.real.as_ref().unwrap().cash;
        crypto_bros::live_api::start_watch(&mut s).unwrap();
        assert!(!s.live_enabled);
        assert!(cards(&s).is_empty(), "defect {defect}");
        s.live_enabled = true; // Advanced Enable cannot bypass the model gate either.
        assert_eq!(crypto_bros::live_api::advice(&s)["action"], "wait");
        assert!(cards(&s).is_empty());
        for (price, risk) in [(112.0, false), (89.0, true)] {
            s.price = price;
            let exits = cards(&s);
            assert_eq!(exits.len(), 1);
            assert_eq!(exits[0]["side"], "sell");
            assert_eq!(exits[0]["risk"], risk);
            assert!(
                watch::order_payload(&s, exits[0]["key"].as_str().unwrap(), s.updated_at).is_ok()
            );
        }
        assert_eq!(s.real.as_ref().unwrap().cash, cash);
        assert_eq!(s.real.as_ref().unwrap().layers[0].quantity, SOL / 10);
    }
}

#[tokio::test]
async fn legacy_model_preparation_and_old_buy_notification_fail_before_network_access() {
    let dir = tempfile::tempdir().unwrap();
    let state = service(dir.path());
    let key = {
        let mut e = state.engine.lock().await;
        let key = cards(&e.snapshot)[0]["key"].clone();
        e.snapshot.real.as_mut().unwrap().advice_research = None;
        key
    };
    let result = crypto_bros::live_api::dispatch(
        &state,
        "live_prepare",
        &serde_json::json!({"side":"buy", "amount_usdc":"2"}),
    )
    .await;
    assert!(result.unwrap_err().contains("หลักฐาน"));
    let result = crypto_bros::live_api::dispatch(
        &state,
        "live_watch_prepare",
        &serde_json::json!({"key":key}),
    )
    .await;
    assert!(result.unwrap_err().contains("สัญญาณนี้"));
    assert!(
        state
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
async fn stale_model_approval_is_blocked_at_preview_and_submit_without_blocking_manual_or_sell() {
    use axum::{
        Json,
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode, header},
    };
    use crypto_bros::{
        live::PendingOrder,
        live_api::{TicketRequest, preview, submit},
    };
    let dir = tempfile::tempdir().unwrap();
    let state = service(dir.path());
    {
        let mut e = state.engine.lock().await;
        let a = e.snapshot.real.as_mut().unwrap();
        a.advice_research = None;
        a.pending = Some(PendingOrder {
            id: "isolated-order".into(),
            side: "buy".into(),
            layer_id: 0,
            input: USDC,
            expected_output: SOL / 100,
            minimum_output: SOL / 100,
            unsigned_transaction: String::new(),
            request_id: String::new(),
            last_valid_block_height: 0,
            prepared_at: chrono::Utc::now().timestamp(),
            expires_at: chrono::Utc::now().timestamp() + 120,
            signature: None,
            status: "prepared".into(),
            network_fee: 5000,
            rent_delta: 0,
            rate: ThbRate {
                micros: 35_000_000,
                fetched_at: chrono::Utc::now().timestamp(),
                source: "fixture".into(),
            },
            price: 100 * USDC,
            target_price: 110 * USDC,
            review_price: 90 * USDC,
            model: "isolated-fixture".into(),
            message: None,
        });
        e.live_ticket = Some((
            "isolated-ticket".into(),
            "isolated-order".into(),
            chrono::Utc::now().timestamp() + 120,
        ));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://127.0.0.1:0"),
    );
    let request = || {
        serde_json::from_value::<TicketRequest>(serde_json::json!({"ticket":"isolated-ticket"}))
            .unwrap()
    };
    let (status, body) = preview(State(state.clone()), headers.clone(), Json(request())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.0["error"].as_str().unwrap().contains("หลักฐาน"));
    let (status, body) = submit(State(state.clone()), headers.clone(), Json(request())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.0["error"].as_str().unwrap().contains("หลักฐาน"));
    // Explicit manual orders retain their original authorization semantics.
    state
        .engine
        .lock()
        .await
        .snapshot
        .real
        .as_mut()
        .unwrap()
        .pending
        .as_mut()
        .unwrap()
        .model = "manual-live".into();
    assert_eq!(
        preview(State(state.clone()), headers.clone(), Json(request()))
            .await
            .0,
        StatusCode::OK
    );
    {
        let mut e = state.engine.lock().await;
        let p = e.snapshot.real.as_mut().unwrap().pending.as_mut().unwrap();
        p.side = "sell".into();
        p.model = "old-model".into();
        e.snapshot.live_enabled = false;
    }
    assert_eq!(
        preview(State(state.clone()), headers, Json(request()))
            .await
            .0,
        StatusCode::OK
    );
    let e = state.engine.lock().await;
    let a = e.snapshot.real.as_ref().unwrap();
    assert!(a.pending.as_ref().unwrap().signature.is_none());
    assert!(a.fills.is_empty());
}
