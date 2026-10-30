//! Read-only wallet valuation. External gas deposits never enlarge the trading ledger.
use crate::{
    live::{SOL, ThbRate, ratio},
    state::Snapshot,
};
use crypto_bros_core::ledger::price_units;
use serde_json::{Value, json};

pub fn view(s: &Snapshot, now: i64) -> Value {
    let Some(a) = s.real.as_ref() else {
        return Value::Null;
    };
    let Some(b) = s.real_balances.as_ref() else {
        return json!({"available":false});
    };
    let price = if s.source == "binance_solusdc" && s.fresh() && s.market_error.is_none() {
        price_units(s.price).ok()
    } else {
        None
    };
    let rate = s.thb_rate.as_ref().filter(|r| r.fresh(now));
    let tracked = a
        .layers
        .iter()
        .try_fold(0u64, |sum, l| sum.checked_add(l.quantity));
    let gas = tracked.map(|q| b.sol.saturating_sub(q));
    let sol_value = price.and_then(|p| ratio(b.sol, p, SOL).ok());
    let gas_value = price.zip(gas).and_then(|(p, q)| ratio(q, p, SOL).ok());
    let total = price
        .and_then(|p| ratio(b.wrapped_sol, p, SOL).ok())
        .zip(sol_value)
        .and_then(|(w, n)| b.usdc.checked_add(w)?.checked_add(n));
    let baht = |value: Option<u64>| {
        value
            .zip(rate)
            .and_then(|(v, r): (_, &ThbRate)| r.satang(v).ok())
    };
    let fees = a
        .fills
        .iter()
        .try_fold(0u64, |sum, f| sum.checked_add(f.network_fee_lamports));
    let rent: i128 = a.fills.iter().map(|f| f.rent_delta_lamports as i128).sum();
    json!({"available":true,"fresh":(0..120).contains(&(now-b.checked_at))&&price.is_some()&&rate.is_some()&&s.watch.error.is_none(),
        "checked_at":b.checked_at,"native_sol":b.sol,"wrapped_sol":b.wrapped_sol,"usdc":b.usdc,
        "tracked_sol":tracked,"gas_sol":gas,"gas_floor":a.config.sol_gas_reserve,
        "gas_low":gas.is_none_or(|g|g<a.config.sol_gas_reserve),
        "sol_satang":baht(sol_value),"gas_satang":baht(gas_value),
        "wallet_usdc":total,"wallet_satang":baht(total),
        "network_fee_sol":fees,"network_fee_usdc":a.fees_usdc,"network_fee_satang":a.fees_satang,
        "net_rent_sol":rent,"open_layers":a.layers.iter().filter(|l|l.quantity>0).count()})
}
