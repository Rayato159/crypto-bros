use crypto_bros::{
    chain::Balances,
    live::{LiveConfig, PendingOrder, RealAccount, SOL, ThbRate, USDC},
    watch,
};

fn fixture() -> (RealAccount, Balances, ThbRate) {
    let now = chrono::Utc::now().timestamp();
    let rate = ThbRate {
        micros: 35_000_000,
        fetched_at: now,
        source: "synthetic fixture".into(),
    };
    let account = RealAccount::new(
        "synthetic-unfunded".into(),
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
    let balances = Balances {
        sol: SOL,
        usdc: 100 * USDC,
        wrapped_sol: 0,
        rent: 0,
        checked_at: now,
    };
    (account, balances, rate)
}

fn order(rate: &ThbRate, side: &str, input: u64, output: u64) -> PendingOrder {
    PendingOrder {
        id: "synthetic-order".into(),
        side: side.into(),
        layer_id: 1,
        input,
        expected_output: output,
        minimum_output: output,
        unsigned_transaction: String::new(),
        request_id: String::new(),
        last_valid_block_height: 0,
        prepared_at: rate.fetched_at,
        expires_at: rate.fetched_at + 60,
        signature: None,
        status: "fixture".into(),
        network_fee: 5000,
        rent_delta: 0,
        rate: rate.clone(),
        price: 100 * USDC,
        target_price: 110 * USDC,
        review_price: 90 * USDC,
        model: "synthetic-manual".into(),
        message: None,
    }
}

#[test]
fn original_funding_is_apportioned_once_and_imports_are_not_trade_fills_or_bot_profit() {
    let (mut a, b, rate) = fixture();
    let before_pnl = a.equity(100 * USDC).unwrap() as i128 - a.initial_usdc as i128;
    a.allocate_80_20(
        &b,
        SOL,
        100 * USDC,
        100 * USDC,
        USDC,
        &rate,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.config.reserve, 20 * USDC);
    assert_eq!(a.layers[0].quantity, SOL * 8 / 10);
    assert_eq!(a.layers[0].reserved_quantity, 0); // The other 20% stays outside the trading ledger for gas.
    assert_eq!(a.layers[0].cost_usdc, 80 * USDC);
    assert_eq!(a.layers[0].cost_satang, 200_000);
    assert_eq!(a.initial_cost_satang, 450_000); // 2500 + 2000 baht, not the full original 5000 twice.
    assert_eq!(a.initial_usdc, 180 * USDC);
    assert_eq!(
        a.equity(100 * USDC).unwrap() as i128 - a.initial_usdc as i128,
        before_pnl
    );
    assert_eq!(a.config.trading_budget, 160 * USDC);
    assert_eq!(a.config.max_buy, 80 * USDC);
    assert!(a.fills.is_empty());
    let stored = serde_json::to_value(&a).unwrap();
    assert!(
        a.allocate_80_20(
            &b,
            SOL,
            100 * USDC,
            100 * USDC,
            USDC,
            &rate,
            rate.fetched_at
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&a).unwrap(), stored);
}

#[test]
fn remaining_sol_keeps_proportional_original_basis_and_a_fee_buffer() {
    let (mut a, mut b, rate) = fixture();
    b.sol = SOL * 9 / 10; // Some original SOL is no longer present.
    a.allocate_80_20(
        &b,
        SOL,
        100 * USDC,
        100 * USDC,
        USDC,
        &rate,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.layers[0].quantity, SOL * 72 / 100);
    assert_eq!(a.layers[0].cost_usdc, 72 * USDC);
    let (mut a, mut b, rate) = fixture();
    b.sol = 10_000_000;
    a.allocate_80_20(
        &b,
        10_000_000,
        USDC,
        100 * USDC,
        USDC,
        &rate,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.layers[0].quantity, 4_000_000);
    assert_eq!(
        b.sol - a.layers[0].quantity,
        a.config.sol_gas_reserve + a.config.max_network_fee
    );
}

#[test]
fn unknown_sol_can_stay_for_fees_then_be_recorded_once_without_releasing_existing_reserves() {
    let (mut a, b, rate) = fixture();
    a.allocate_80_20(&b, 0, 0, 100 * USDC, USDC, &rate, rate.fetched_at)
        .unwrap();
    assert_eq!(a.config.reserve, 20 * USDC);
    assert!(a.layers.is_empty());
    assert!(a.sol_funding.is_none());
    a.config.reserve = 50 * USDC; // Previously reserved proceeds are never reallocated away.
    a.allocate_80_20(
        &b,
        SOL,
        100 * USDC,
        100 * USDC,
        USDC,
        &rate,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.config.reserve, 50 * USDC);
}

#[test]
fn every_receipt_is_split_and_repeated_partial_sales_cannot_spend_protected_sol() {
    let (mut a, b, rate) = fixture();
    a.allocate_80_20(&b, 0, 0, 100 * USDC, USDC, &rate, rate.fetched_at)
        .unwrap();
    let buy = order(&rate, "buy", 10 * USDC, SOL / 10);
    a.check_buy(buy.input, buy.price).unwrap();
    a.apply_fill(
        &buy,
        "synthetic-buy",
        buy.input,
        buy.minimum_output,
        5000,
        0,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.layers[0].reserved_quantity, SOL / 50);
    assert_eq!(a.layers[0].tradable_quantity(), SOL * 8 / 100);
    let mut sale = order(&rate, "sell", SOL * 4 / 100, 4_400_000);
    a.apply_fill(
        &sale,
        "synthetic-sell-1",
        sale.input,
        sale.minimum_output,
        5000,
        0,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.config.reserve, 20_880_000);
    a.apply_fill(
        &sale,
        "synthetic-sell-2",
        sale.input,
        sale.minimum_output,
        5000,
        0,
        rate.fetched_at,
    )
    .unwrap();
    assert_eq!(a.config.reserve, 21_760_000);
    assert_eq!(a.layers[0].quantity, SOL / 50);
    assert_eq!(a.layers[0].tradable_quantity(), 0);
    assert_eq!(a.layers[0].cost_usdc, 2 * USDC);
    assert_eq!(a.cash, 98_800_000);
    let after = serde_json::to_value(&a).unwrap();
    sale.input = SOL / 100;
    assert!(
        a.apply_fill(
            &sale,
            "synthetic-illegal-sale",
            sale.input,
            sale.minimum_output,
            5000,
            0,
            rate.fetched_at
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&a).unwrap(), after);
    let restored: RealAccount = serde_json::from_value(after).unwrap();
    assert_eq!(restored.layers[0].tradable_quantity(), 0);
    assert_eq!(restored.config.reserve, 21_760_000);
}

#[test]
fn optimized_sizes_remain_bounded_and_partial_profit_exits_cover_both_currencies() {
    let (mut a, b, rate) = fixture();
    a.allocate_80_20(&b, 0, 0, 100 * USDC, USDC, &rate, rate.fetched_at)
        .unwrap();
    a.advice_policy.buy_size_bps = 5000;
    assert_eq!(watch::buy_size(&a), 20 * USDC); // Half of the 40-USDC slot cap, not half the whole wallet.
    a.advice_policy.buy_size_bps = u16::MAX;
    assert_eq!(watch::buy_size(&a), 40 * USDC);
    assert!(a.check_buy(81 * USDC, 100 * USDC).is_err());
    let buy = order(&rate, "buy", 10 * USDC, SOL / 10);
    a.apply_fill(
        &buy,
        "synthetic-buy",
        buy.input,
        buy.minimum_output,
        5000,
        0,
        rate.fetched_at,
    )
    .unwrap();
    a.advice_policy.sell_size_bps = 5000;
    let qty = a.advice_policy.sell_size(a.layers[0].tradable_quantity());
    assert_eq!(qty, SOL * 4 / 100);
    assert!(watch::profit_exit_covers_costs(
        &a.layers[0],
        qty,
        4_400_000,
        5000,
        100 * USDC,
        &rate
    ));
    let low_fx = ThbRate {
        micros: 30_000_000,
        ..rate.clone()
    };
    assert!(!watch::profit_exit_covers_costs(
        &a.layers[0],
        qty,
        4_400_000,
        5000,
        100 * USDC,
        &low_fx
    ));
    assert!(!watch::profit_exit_covers_costs(
        &a.layers[0],
        a.layers[0].quantity,
        11 * USDC,
        5000,
        100 * USDC,
        &rate
    ));
}

#[test]
fn invalid_capital_changes_leave_the_account_untouched() {
    for defect in 0..5 {
        let (mut a, mut b, rate) = fixture();
        let mut cost = 100 * USDC;
        let mut received = SOL;
        match defect {
            0 => cost = 101 * USDC,
            1 => received = 0,
            2 => b.checked_at -= 121,
            3 => b.sol = 5_000_000,
            _ => a.pending = Some(order(&rate, "buy", USDC, SOL / 100)),
        }
        let before = serde_json::to_value(&a).unwrap();
        assert!(
            a.allocate_80_20(&b, received, cost, 100 * USDC, USDC, &rate, rate.fetched_at)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&a).unwrap(), before);
    }
}
