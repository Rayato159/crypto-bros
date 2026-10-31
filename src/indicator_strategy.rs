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

pub fn check_quote(order: &crate::live::PendingOrder) -> Result<(), &'static str> {
    if order.input == 0 || order.minimum_output == 0 || order.target_price <= order.price {
        return Err("ราคาเป้าหมายหรือจำนวนคำสั่งไม่ถูกต้อง");
    }
    let gross =
        order.minimum_output as u128 * order.target_price as u128 / crate::live::SOL as u128;
    // Entry cost is already represented by the route's minimum received SOL.
    // Exit cost remains an estimate (30 bps + another network fee), not a quote.
    let exit = gross * 9970 / 10_000;
    let network = 2 * order.network_fee as u128 * order.price as u128 / crate::live::SOL as u128;
    let required = (order.input as u128 * 10_020).div_ceil(10_000);
    if exit.saturating_sub(network) <= required {
        return Err(
            "ราคาเป้าหมายยังไม่เหลือส่วนต่าง 0.20% หลังราคาซื้อจริงและค่าซื้อขายโดยประมาณ จึงยังไม่เตรียมซื้อ เป้านี้ไม่ใช่คำทำนายว่าจะไปถึง",
        );
    }
    Ok(())
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
