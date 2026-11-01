//! Read-only trade suggestions. Every actionable card is revalidated before quoting.
use crate::{
    live::{RealAccount, SOL, USDC},
    state::Snapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const REMINDER_SECONDS: i64 = 15 * 60;

pub fn profit_exit_covers_costs(
    layer: &crate::live::RealLayer,
    quantity: u64,
    minimum_usdc: u64,
    fee_sol: u64,
    price: u64,
    rate: &crate::live::ThbRate,
) -> bool {
    if quantity == 0 || quantity > layer.tradable_quantity() {
        return false;
    }
    let Ok(fee) = crate::live::ratio(fee_sol, price, SOL) else {
        return false;
    };
    let net = minimum_usdc.saturating_sub(fee);
    let (Ok(cost), Ok(cost_thb), Ok(net_thb)) = (
        crate::live::ratio(
            layer.cost_usdc.saturating_add(layer.entry_fee_usdc),
            quantity,
            layer.quantity,
        ),
        crate::live::ratio(
            layer.cost_satang.saturating_add(layer.entry_fee_satang),
            quantity,
            layer.quantity,
        ),
        rate.satang(net),
    ) else {
        return false;
    };
    net > cost && net_thb > cost_thb
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Watch {
    pub enabled: bool,
    pub snoozed: BTreeMap<String, i64>,
    pub notified: BTreeMap<String, i64>,
    pub error: Option<String>,
    #[serde(default)]
    pub last_buy_quote: Option<crate::indicator_strategy::BuyQuoteReview>,
}

pub fn buy_size(a: &RealAccount) -> u64 {
    let open = a
        .layers
        .iter()
        .filter(|l| l.tradable_quantity() > 0)
        .count();
    let slots = a.config.max_layers.saturating_sub(open);
    if slots == 0 {
        return 0;
    }
    let exposure = a
        .layers
        .iter()
        .fold(0_u64, |sum, l| sum.saturating_add(l.cost_usdc));
    let remaining = a.config.trading_budget.saturating_sub(exposure);
    let bounded = a
        .cash
        .saturating_sub(a.config.reserve)
        .saturating_sub(a.parked_profit)
        .min(a.config.max_buy)
        .min(remaining / slots as u64);
    a.entry_policy().buy_size(bounded)
}

fn status(s: &Snapshot, state: &str, message: &str) -> Value {
    json!({"enabled":s.watch.enabled,"status":state,"message":message,"signals":[],"checked_at":s.updated_at})
}

pub fn view(s: &Snapshot, now: i64) -> Value {
    if !s.watch.enabled || s.updating {
        return status(s, "paused", "หยุดเฝ้าตลาดอยู่ กดเริ่มเมื่อต้องการรับสัญญาณ");
    }
    let Some(a) = s.real.as_ref() else {
        return status(s, "setup", "เชื่อมกระเป๋าบอตและบันทึกทุนก่อน");
    };
    if a.pending.is_some() {
        return status(s, "pending", "มีรายการรอยืนยันหรือรอตรวจผล จัดการรายการเดิมก่อน");
    }
    if a.reconciliation_error.is_some() {
        return status(s, "blocked", "ยอดบัญชียังตรวจสอบไม่ลงตัว หยุดเสนอรายการใหม่");
    }
    if s.source != "binance_solusdc"
        || !s.fresh()
        || s.market_error.is_some()
        || !(0..120).contains(&(now - s.updated_at))
    {
        return status(
            s,
            "unavailable",
            "ข้อมูลราคาขาดหายหรือเก่าเกินไป ยังเสนอซื้อขายไม่ได้",
        );
    }
    let Some(rate) = s.thb_rate.as_ref().filter(|r| r.fresh(now)) else {
        return status(s, "unavailable", "กำลังรออัตราเงินบาทล่าสุด ยังเสนอซื้อขายไม่ได้");
    };
    let Some(balances) = s
        .real_balances
        .as_ref()
        .filter(|b| (0..120).contains(&(now - b.checked_at)))
    else {
        return status(s, "unavailable", "กำลังรอตรวจยอดกระเป๋าล่าสุด ยังเสนอซื้อขายไม่ได้");
    };
    if let Some(error) = &s.watch.error {
        return status(s, "unavailable", error);
    }
    let Ok(price) = crypto_bros_core::ledger::price_units(s.price) else {
        return status(s, "unavailable", "ราคาไม่ถูกต้อง");
    };
    let advice = crate::live_api::advice(s);
    let global_risk = advice["risk_blocked"] == true;
    let mut signals = Vec::new();
    // Risk exits precede profit exits, including when new buys are paused.
    for l in a.layers.iter().filter(|l| l.tradable_quantity() > 0) {
        let risk = global_risk || price <= l.review_price;
        if !risk && price < l.target_price {
            continue;
        }
        let reason = if risk { "risk" } else { "target" };
        let quantity = if risk || a.advice_eligibility().is_err() {
            l.tradable_quantity()
        } else {
            a.entry_policy().sell_size(l.tradable_quantity())
        };
        if quantity == 0 {
            continue;
        }
        let key = format!("sell:{reason}:{}:{quantity}", l.id);
        let value = crate::live::ratio(quantity, price, SOL).unwrap_or(0);
        let basis = crate::live::ratio(
            l.cost_usdc.saturating_add(l.entry_fee_usdc),
            quantity,
            l.quantity,
        )
        .unwrap_or(u64::MAX);
        let basis_thb = crate::live::ratio(
            l.cost_satang.saturating_add(l.entry_fee_satang),
            quantity,
            l.quantity,
        )
        .unwrap_or(u64::MAX);
        let pnl = value as i128 - basis as i128;
        let value_thb = rate.satang(value).unwrap_or(0);
        let pnl_thb = value_thb as i128 - basis_thb as i128;
        let below_basis = !risk && (pnl <= 0 || pnl_thb <= 0);
        signals.push(json!({"key":key,"side":"sell","risk":risk,"layer_id":l.id,
            "title":if risk {format!("ทบทวนขายไม้ #{} เพื่อลดความเสี่ยง",l.id)} else if below_basis {format!("ไม้ #{} ถึงราคาเป้า แต่ยังไม่คุ้มต้นทุน",l.id)} else {format!("ไม้ #{} ถึงราคาเป้าหมายแล้ว",l.id)},
            "reason":if risk {"ถึงเกณฑ์ทบทวนความเสี่ยงของไม้หรือเงินทุน หยุดซื้อเพิ่มและตรวจรายการขาย"} else {"ราคาตลาดถึงเป้าหมายขายที่บันทึกไว้ตอนเข้าไม้"},
            "quantity_sol":quantity,"amount_usdc":value,"value_satang":value_thb,
            "pnl_usdc":pnl,"pnl_satang":pnl_thb,"below_cost_basis":below_basis,
            "button":"ตรวจราคาขาย → Phantom","observed_at":s.updated_at,"expires_at":s.updated_at+120}));
    }
    signals.sort_by_key(|v| v["risk"] != true);
    let amount = buy_size(a);
    let cooldown = a
        .fills
        .iter()
        .filter(|f| f.side == "buy")
        .any(|f| now.saturating_sub(f.time) < a.entry_policy().cooldown_bars as i64 * 900);
    let held_sol = a
        .layers
        .iter()
        .fold(0_u64, |sum, l| sum.saturating_add(l.quantity));
    let gas_ready = balances.sol > held_sol.saturating_add(a.config.sol_gas_reserve);
    let balance_ready = balances.usdc >= a.cash && balances.wrapped_sol == 0;
    if signals.is_empty()
        && advice["action"] == "buy"
        && !cooldown
        && gas_ready
        && balance_ready
        && a.check_buy(amount, price).is_ok()
    {
        let key = format!(
            "buy:{}:{}",
            if a.entry_strategy == crate::indicator_strategy::EntryStrategy::RsiMacd {
                format!(
                    "{}:{}",
                    crate::indicator_strategy::VERSION,
                    s.candles.last().map_or(0, |c| c.time)
                )
            } else {
                a.advice_model
                    .as_ref()
                    .map_or("none", |m| m.version.as_str())
                    .to_string()
            },
            a.fills.len()
        );
        let cost_blocked = s.watch.last_buy_quote.as_ref().is_some_and(|q| {
            q.watch_key.as_deref() == Some(key.as_str())
                && q.input_usdc == amount
                && q.reference_price == price
                && q.expires_at > now
                && !q.passed
        });
        signals.push(json!({"key":key,"side":"buy","risk":false,"layer_id":0,
            "cost_blocked":cost_blocked,"quote_required":true,
            "title":if cost_blocked {"สัญญาณผ่าน แต่ราคาที่ตรวจล่าสุดยังไม่คุ้มค่าใช้จ่าย".into()}
                else {format!("สัญญาณผ่าน · รอตรวจราคาซื้อ {:.2} USDC",amount as f64/USDC as f64)},
            "reason":advice["reason"],
            "amount_usdc":amount,"value_satang":rate.satang(amount).unwrap_or(0),
            "button":if cost_blocked {"ตรวจราคาใหม่"}else{"ตรวจราคาซื้อก่อนยืนยัน"},"observed_at":s.updated_at,"expires_at":s.updated_at+120}));
    }
    signals.retain(|v| {
        s.watch
            .snoozed
            .get(v["key"].as_str().unwrap_or(""))
            .is_none_or(|until| *until <= now)
    });
    let message = if !signals.is_empty() {
        "มีรายการให้ตรวจสอบ ก่อนส่งต้องยืนยันใน Phantom"
    } else if global_risk {
        "มีเงื่อนไขความเสี่ยง หยุดซื้อเพิ่ม; รายการที่พักไว้จะแจ้งอีกครั้งเมื่อครบเวลา"
    } else if let Err(reason) = crate::live_api::model_buy_eligibility(s) {
        reason
    } else if !s.live_enabled {
        "ยังเฝ้าราคาขายอยู่ แต่พักการเสนอซื้อเพิ่ม"
    } else if a.entry_strategy == crate::indicator_strategy::EntryStrategy::ResearchModel
        && a.advice_model.is_none()
    {
        "ยังไม่มีสูตรแนะนำซื้อ กดเริ่มเฝ้าตลาดเพื่อเตรียมสูตร"
    } else if !gas_ready || !balance_ready {
        "ยอดกระเป๋าหรือ SOL สำหรับค่าธรรมเนียมยังไม่พร้อมซื้อ"
    } else if amount == 0 || a.check_buy(amount, price).is_err() {
        "เฝ้าไม้ที่ถืออยู่ วงเงินหรือจำนวนไม้ยังไม่เปิดให้ซื้อเพิ่ม"
    } else if cooldown {
        "เฝ้าตลาดอยู่ รอครบช่วงเว้นระยะหลังซื้อไม้ก่อน"
    } else if a.entry_strategy == crate::indicator_strategy::EntryStrategy::RsiMacd {
        "เฝ้า RSI/MACD อยู่ ดูเงื่อนไขที่ยังไม่ผ่านด้านล่าง ไม่ต้องรอผล Research"
    } else {
        "เฝ้าตลาดอยู่ ยังไม่มีรายการที่ผ่านเกณฑ์ให้กด"
    };
    json!({"enabled":true,"status":if signals.is_empty(){"watching"}else{"actionable"},"message":message,
        "signals":signals,"checked_at":s.updated_at,"planned_buy_usdc":amount})
}

pub fn order_payload(s: &Snapshot, key: &str, now: i64) -> Result<Value, String> {
    let view = view(s, now);
    let signal = view["signals"]
        .as_array()
        .and_then(|list| list.iter().find(|v| v["key"] == key))
        .ok_or("สัญญาณนี้หมดอายุ เปลี่ยนไป หรือถูกพักแล้ว รอรายการล่าสุด")?;
    if signal["side"] == "buy" {
        let amount = signal["amount_usdc"].as_u64().ok_or("วงเงินไม่ถูกต้อง")?;
        Ok(
            json!({"side":"buy","amount_usdc":format!("{}.{:06}",amount/USDC,amount%USDC),"watch_key":key}),
        )
    } else {
        Ok(
            json!({"side":"sell","layer_id":signal["layer_id"],"quantity_sol_atomic":signal["quantity_sol"],"watch_key":key}),
        )
    }
}

pub fn claim(s: &mut Snapshot, now: i64) -> Option<Value> {
    let v = view(s, now);
    let signal = v["signals"]
        .as_array()?
        .iter()
        .filter(|v| v["cost_blocked"] != true)
        .find(|v| {
            s.watch
                .notified
                .get(v["key"].as_str().unwrap_or(""))
                .is_none_or(|time| now - *time >= REMINDER_SECONDS)
        })?
        .clone();
    s.watch.notified.retain(|_, time| now - *time < 24 * 3600);
    s.watch.notified.insert(signal["key"].as_str()?.into(), now);
    Some(signal)
}
