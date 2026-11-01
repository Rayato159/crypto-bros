//! Transparent, manually approved entry rules and realized-outcome memory.
//! This is not a validated forecast model or Q-learning. Rewards never rewrite
//! entry thresholds, reserves or limits, and unrealized gains are not rewards.
use crypto_bros_core::indicators::{Candle, calculate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const VERSION: &str = "rsi-macd-v1";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStrategy {
    #[default]
    ResearchModel,
    RsiMacd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryContext {
    pub version: String,
    pub candle_time: i64,
    pub close: f64,
    pub rsi: f64,
    pub previous_rsi: f64,
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
    pub previous_histogram: f64,
    pub atr: f64,
}

impl EntryContext {
    pub fn buy_ready(&self) -> bool {
        self.version == VERSION
            && [
                self.close,
                self.rsi,
                self.previous_rsi,
                self.macd,
                self.signal,
                self.histogram,
                self.previous_histogram,
                self.atr,
            ]
            .iter()
            .all(|v| v.is_finite())
            && self.close > 0.0
            && self.atr > 0.0
            && (30.0..=55.0).contains(&self.rsi)
            && self.rsi > self.previous_rsi
            && self.histogram > 0.0
            && self.histogram > self.previous_histogram
            && self.macd > self.signal
    }

    // Fixed bins make the definition of "similar" auditable. They are not
    // selected by looking for profitable outcomes in this user's history.
    fn bucket(&self) -> (i32, bool, i32) {
        (
            (self.rsi / 5.0).floor() as i32,
            self.macd >= 0.0,
            ((self.histogram / self.atr) * 10.0).floor().clamp(0.0, 9.0) as i32,
        )
    }
}

pub fn context(candles: &[Candle], now: i64) -> Result<EntryContext, &'static str> {
    let last = candles.last().ok_or("กำลังรอแท่งราคาสำหรับ RSI/MACD")?;
    if candles.len() < 51
        || !candles.iter().all(Candle::valid)
        || candles.windows(2).any(|w| w[1].time - w[0].time != 900)
        || !(900..1800).contains(&(now - last.time))
    {
        return Err("แท่งราคา RSI/MACD ยังไม่ปิด เก่า หรือขาดช่วง กำลังรอข้อมูลใหม่");
    }
    let features = calculate(candles);
    let current = features
        .last()
        .and_then(Option::as_ref)
        .ok_or("RSI/MACD ยังมีข้อมูลไม่พอ")?;
    let previous = features
        .get(features.len() - 2)
        .and_then(Option::as_ref)
        .ok_or("RSI/MACD ยังมีข้อมูลไม่พอ")?;
    Ok(EntryContext {
        version: VERSION.into(),
        candle_time: last.time,
        close: last.close,
        rsi: current.rsi,
        previous_rsi: previous.rsi,
        macd: current.macd,
        signal: current.signal,
        histogram: current.histogram,
        previous_histogram: previous.histogram,
        atr: current.atr,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardTrade {
    pub layer_id: u64,
    pub entry: EntryContext,
    pub sold_sol: u64,
    pub disposed_cost_usdc: u64,
    pub net_usdc: i64,
    pub net_satang: i64,
    pub exit_fills: usize,
    pub complete: bool,
    pub failed_entry: bool,
}

pub fn memory(trades: &[RewardTrade], entry: Option<&EntryContext>) -> Value {
    let similar: Vec<_> = trades
        .iter()
        .filter(|t| {
            t.entry.version == VERSION && entry.is_none_or(|e| e.bucket() == t.entry.bucket())
        })
        .collect();
    let completed: Vec<_> = similar
        .iter()
        .filter(|t| t.complete && !t.failed_entry)
        .collect();
    let count = completed.len();
    let net: i128 = similar.iter().map(|t| t.net_usdc as i128).sum();
    let thb: i128 = similar.iter().map(|t| t.net_satang as i128).sum();
    let wins = completed
        .iter()
        .filter(|t| t.net_usdc > 0 && t.net_satang > 0)
        .count();
    let score: f64 = completed
        .iter()
        .map(|t| t.net_usdc as f64 / t.disposed_cost_usdc.max(1) as f64)
        .sum::<f64>()
        / count.max(1) as f64;
    json!({"completed":count,"wins":wins,"other_outcomes":count-wins,
        "net_usdc":net,"net_satang":thb,"mean_reward":if count>0 {Some(score)}else{None},
        "partial":similar.iter().filter(|t| !t.complete && t.exit_fills>0).count(),
        "failed_entries":similar.iter().filter(|t|t.failed_entry).count(),
        "changes_rules":false})
}

/// Hypothetical disposal of the tradable 80%, at its unchanged ATR target.
/// No reserved SOL is sold or counted as realized profit. These are estimates,
/// not forecasts or executable exit quotes; rent remains a recoverable deposit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitEstimate {
    pub received_sol: u64,
    pub reserved_sol: u64,
    pub tradable_sol: u64,
    pub entry_price: u64,
    pub target_price: u64,
    pub cost_usdc: u64,
    pub entry_fee_usdc: u64,
    pub exit_swap_cost_usdc: u64,
    pub exit_network_cost_usdc: u64,
    pub net_usdc: i64,
    pub required_net_usdc: u64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyQuoteReview {
    pub checked_at: i64,
    pub expires_at: i64,
    pub watch_key: Option<String>,
    pub reference_price: u64,
    pub input_usdc: u64,
    pub network_fee_lamports: u64,
    pub rent_delta_lamports: i64,
    pub reserve_bps: u16,
    pub exit_fee_bps: u16,
    pub expected: ExitEstimate,
    pub minimum: ExitEstimate,
    pub passed: bool,
}

impl BuyQuoteReview {
    pub fn ensure_passes(&self) -> Result<(), String> {
        if self.passed {
            return Ok(());
        }
        let limiting = if self.minimum.net_usdc <= self.expected.net_usdc {
            &self.minimum
        } else {
            &self.expected
        };
        Err(format!(
            "สัญญาณผ่าน แต่ราคาที่ตรวจยังไม่คุ้ม: ซื้อ {:.4} USDC แล้วขายเฉพาะส่วนเทรดที่เป้า คาดเหลือสุทธิ {:+.6} USDC ต้องมากกว่า {:.6} USDC (0.20% ของต้นทุนส่วนที่ขาย) ดูผลตรวจราคาใต้การ์ด ยังไม่ได้ส่งคำสั่ง",
            self.input_usdc as f64 / 1e6,
            limiting.net_usdc as f64 / 1e6,
            limiting.required_net_usdc as f64 / 1e6,
        ))
    }
}

fn estimate_exit(order: &crate::live::PendingOrder, received: u64) -> Result<ExitEstimate, String> {
    use crate::live::{SOL, ratio, reserved_part};
    let reserved = reserved_part(received, 2000);
    let tradable = received.saturating_sub(reserved);
    if tradable == 0 {
        return Err("ไม่มี SOL ส่วนเทรดหลังกันสำรอง".into());
    }
    let (target, _) = order.entry_thresholds(received)?;
    let gross = ratio(tradable, target, SOL)?;
    let proceeds = ratio(gross, 9970, 10_000)?;
    let cost = ratio(order.input, tradable, received)?;
    let entry_fee = ratio(
        ratio(order.network_fee, order.price, SOL)?,
        tradable,
        received,
    )?;
    // Match settlement's fee valuation at the prospective exit price. The future
    // network fee is unknown: use the quoted entry fee's lamports as an estimate.
    let exit_network = ratio(order.network_fee, target, SOL)?;
    let net =
        i64::try_from(proceeds as i128 - cost as i128 - entry_fee as i128 - exit_network as i128)
            .map_err(|_| "Quote cost overflow")?;
    let required = u64::try_from((cost as u128 * 20).div_ceil(10_000))
        .map_err(|_| "Quote threshold overflow")?;
    Ok(ExitEstimate {
        received_sol: received,
        reserved_sol: reserved,
        tradable_sol: tradable,
        entry_price: ratio(order.input, SOL, received)?,
        target_price: target,
        cost_usdc: cost,
        entry_fee_usdc: entry_fee,
        exit_swap_cost_usdc: gross - proceeds,
        exit_network_cost_usdc: exit_network,
        net_usdc: net,
        required_net_usdc: required,
        passed: net as i128 > required as i128,
    })
}

pub fn quote_review(order: &crate::live::PendingOrder) -> Result<BuyQuoteReview, String> {
    use crate::live::{SOL, ratio};
    if order.side != "buy"
        || order.input == 0
        || order.minimum_output == 0
        || order.minimum_output > order.expected_output
        || order.target_price <= order.price
        || order.price == 0
    {
        return Err("ราคาเป้าหมายหรือจำนวนคำสั่งไม่ถูกต้อง".into());
    }
    // Re-anchoring the target must never bypass the existing 2% route bound.
    let fair = ratio(order.input, SOL, order.price)?;
    if order.minimum_output as u128 * 100 < fair as u128 * 98 {
        return Err("ราคาซื้อคลาดเคลื่อนเกินขอบเขต 2% กรุณาตรวจราคาใหม่".into());
    }
    let expected = estimate_exit(order, order.expected_output)?;
    let minimum = estimate_exit(order, order.minimum_output)?;
    Ok(BuyQuoteReview {
        checked_at: order.prepared_at,
        expires_at: order.expires_at,
        watch_key: None,
        reference_price: order.price,
        input_usdc: order.input,
        network_fee_lamports: order.network_fee,
        rent_delta_lamports: order.rent_delta,
        reserve_bps: 2000,
        exit_fee_bps: 30,
        passed: expected.passed && minimum.passed,
        expected,
        minimum,
    })
}

pub fn check_quote(order: &crate::live::PendingOrder) -> Result<(), String> {
    quote_review(order)?.ensure_passes()
}

pub fn view(s: &crate::state::Snapshot, now: i64) -> Value {
    let Some(a) = &s.real else { return Value::Null };
    let evaluated = context(&s.candles, now);
    let current = evaluated.as_ref().ok();
    json!({"active":a.entry_strategy==EntryStrategy::RsiMacd,"version":VERSION,
        "context":current,"ready":current.is_some_and(|c|c.buy_ready()),
        "error":evaluated.as_ref().err(),
        "rsi_zone":current.is_some_and(|c|(30.0..=55.0).contains(&c.rsi)),
        "rsi_rising":current.is_some_and(|c|c.rsi>c.previous_rsi),
        "macd_above_signal":current.is_some_and(|c|c.histogram>0.0),
        "histogram_rising":current.is_some_and(|c|c.histogram>c.previous_histogram),
        "similar":memory(&a.reward_trades,current),"all":memory(&a.reward_trades,None)})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> EntryContext {
        EntryContext {
            version: VERSION.into(),
            candle_time: 900,
            close: 100.0,
            rsi: 40.0,
            previous_rsi: 38.0,
            macd: -0.1,
            signal: -0.2,
            histogram: 0.1,
            previous_histogram: -0.1,
            atr: 1.0,
        }
    }
    #[test]
    fn low_rsi_alone_cannot_buy_and_each_condition_is_required() {
        assert!(entry().buy_ready());
        for i in 0..6 {
            let mut e = entry();
            match i {
                0 => e.rsi = 20.0,
                1 => e.rsi = 60.0,
                2 => e.previous_rsi = 41.0,
                3 => e.histogram = -0.01,
                4 => e.previous_histogram = 0.2,
                _ => e.atr = f64::NAN,
            };
            assert!(!e.buy_ready());
        }
    }
    #[test]
    fn partial_exits_never_manufacture_completed_reward_samples() {
        let mut t = RewardTrade {
            layer_id: 1,
            entry: entry(),
            sold_sol: 1,
            disposed_cost_usdc: 100,
            net_usdc: 10,
            net_satang: 35,
            exit_fills: 10,
            complete: false,
            failed_entry: false,
        };
        assert_eq!(memory(&[t.clone()], Some(&entry()))["completed"], 0);
        t.complete = true;
        let m = memory(&[t], Some(&entry()));
        assert_eq!(m["completed"], 1);
        assert_eq!(m["wins"], 1);
        assert_eq!(m["mean_reward"], 0.1);
    }
}
