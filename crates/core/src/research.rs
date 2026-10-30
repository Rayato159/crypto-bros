use crate::{
    indicators::{Candle, calculate},
    ledger::{Ledger, USDC, price_units},
    model::{ForecastModel, HORIZON, Policy, samples},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub net_return: f64,
    pub max_drawdown: f64,
    pub trades: usize,
    #[serde(default)]
    pub entries: usize,
    pub review_triggered: bool,
    pub equity_curve: Vec<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Research {
    #[serde(default)]
    pub execution_version: u32,
    pub version: String,
    pub model: ForecastModel,
    pub policy: Policy,
    pub trials: usize,
    pub validation: Metrics,
    pub holdout: Metrics,
    pub baseline: Metrics,
    pub status: String,
    pub trained_through: i64,
    pub holdout_start: i64,
    pub explanation: String,
}

impl Research {
    /// Passing this gate permits human-approved advice, not unattended trading
    /// or a claim of profitability. Unknown/legacy statuses fail closed.
    pub fn live_eligibility(&self) -> Result<(), &'static str> {
        match self.status.as_str() {
            "shadow_candidate" => {}
            "rejected" => return Err("สูตรทดสอบไม่ผ่าน (rejected) ยังใช้เสนอซื้อไม่ได้"),
            "insufficient_evidence" => {
                return Err("หลักฐานทดสอบยังไม่พอ (insufficient_evidence) ยังใช้เสนอซื้อไม่ได้");
            }
            _ => return Err("ไม่รู้จักสถานะผลทดสอบ ยังใช้สูตรนี้เสนอซื้อไม่ได้"),
        }
        if self.execution_version != 2 {
            return Err("ผลทดสอบรุ่นเก่ายังไม่รวมสำรอง 20/80 และขนาดไม้ ต้องทดสอบด้วยข้อมูลชุดใหม่ก่อน");
        }
        // Recheck the recorded evidence rather than trusting a status label.
        if self.holdout.entries < 8
            || self.holdout.trades < self.holdout.entries
            || !self.holdout.net_return.is_finite()
            || self.holdout.net_return <= 0.0
            || !self.holdout.max_drawdown.is_finite()
            || !(0.0..0.1).contains(&self.holdout.max_drawdown)
            || self.version != self.model.version
            || self.trained_through != self.model.training_end
            || !(2500..=10_000).contains(&self.policy.buy_size_bps)
            || !(2500..=10_000).contains(&self.policy.sell_size_bps)
            || !self
                .model
                .weights
                .iter()
                .chain(self.model.means.iter())
                .all(|v| v.is_finite())
            || !self.model.scales.iter().all(|v| v.is_finite() && *v > 0.0)
            || !self.model.uncertainty.is_finite()
            || self.model.uncertainty < 0.0
            || ![
                self.policy.min_edge,
                self.policy.uncertainty_weight,
                self.policy.take_profit_atr,
                self.policy.review_atr,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            || self.policy.fee_bps > 1000
            || self.policy.slippage_bps > 1000
        {
            return Err("หลักฐานทดสอบไม่ผ่านเกณฑ์หรือไม่ตรงกับสูตร ยังใช้เสนอซื้อไม่ได้");
        }
        Ok(())
    }
}

/// Closed-bar decision, next-bar-open execution. No optimistic intrabar fills.
/// A review trigger freezes new entries and holds the position through the end:
/// it never assumes a human approved a stop-loss. All open losses are marked.
pub fn backtest(
    candles: &[Candle],
    model: &ForecastModel,
    policy: &Policy,
    start: usize,
    end: usize,
) -> Metrics {
    let Ok(mut ledger) = Ledger::new(1000 * USDC, 200 * USDC, 2) else {
        return Metrics::default();
    };
    ledger.reserve_bps = 2000;
    let features = calculate(candles);
    let mut metrics = Metrics::default();
    let mut last_buy = 0;
    for i in start.max(50)..end.min(candles.len()) {
        let c = &candles[i];
        let Ok(mark) = price_units(c.close) else {
            continue;
        };
        let previous = &candles[i - 1];
        // Only information available before this bar may veto its opening fill.
        let review = ledger.layers.iter().any(|l| {
            l.tradable_quantity() > 0 && previous.close <= l.review_price as f64 / USDC as f64
        });
        if review {
            metrics.review_triggered = true;
        }
        let sell_ids: Vec<_> = ledger
            .layers
            .iter()
            .filter(|l| {
                !metrics.review_triggered
                    && l.tradable_quantity() > 0
                    && previous.close >= l.target_price as f64 / USDC as f64
            })
            .map(|l| (l.id, policy.sell_size(l.tradable_quantity())))
            .collect();
        for (id, quantity) in sell_ids {
            if let Ok(price) = price_units(c.open * (1.0 - policy.slippage_bps as f64 / 10_000.0)) {
                let _ = ledger.sell(id, quantity, price, policy.fee_bps, c.time);
            }
        }
        if !metrics.review_triggered
            && ledger.open_count() < ledger.max_layers
            && i >= last_buy + policy.cooldown_bars
        {
            if let Some(ind) = &features[i - 1] {
                if policy.net_edge(model.predict(ind.features), model.uncertainty) > policy.min_edge
                {
                    let buy = c.open * (1.0 + policy.slippage_bps as f64 / 10_000.0);
                    if let (Ok(price), Ok(target), Ok(review)) = (
                        price_units(buy),
                        price_units(buy + policy.take_profit_atr * ind.atr),
                        price_units((buy - policy.review_atr * ind.atr).max(0.01)),
                    ) {
                        if ledger
                            .buy(
                                policy.buy_size(
                                    (ledger.available()
                                        / (ledger.max_layers - ledger.open_count()) as u64)
                                        .min(400 * USDC),
                                ),
                                price,
                                policy.fee_bps,
                                c.time,
                                &model.version,
                                target,
                                review,
                            )
                            .is_ok()
                        {
                            last_buy = i;
                        }
                    }
                }
            }
        }
        if let Ok(dd) = ledger.mark(mark) {
            metrics.max_drawdown = metrics.max_drawdown.max(dd);
            if dd >= 0.1 {
                metrics.review_triggered = true;
            }
        }
        if ledger
            .layers
            .iter()
            .any(|l| l.tradable_quantity() > 0 && c.close <= l.review_price as f64 / USDC as f64)
        {
            metrics.review_triggered = true;
        }
        if let Ok(equity) = ledger.equity(mark) {
            metrics.equity_curve.push(equity as f64 / USDC as f64);
        }
    }
    metrics.net_return = metrics
        .equity_curve
        .last()
        .map_or(0.0, |e| e / 1000.0 - 1.0);
    metrics.trades = ledger.fills.len();
    metrics.entries = ledger.fills.iter().filter(|f| f.side == "buy").count();
    metrics
}

/// Bounded, reproducible search on validation only. The final contiguous holdout
/// is never used to choose a trial. A later optimization needs a fresh holdout.
pub fn optimize(candles: &[Candle]) -> Result<Research, String> {
    if candles.len() < 600
        || !candles.iter().all(Candle::valid)
        || candles.windows(2).any(|w| w[1].time - w[0].time != 900)
    {
        return Err("Need at least 600 valid chronological closed 15-minute candles".into());
    }
    let n = candles.len();
    let train_end = n * 55 / 100;
    let calibration_end = n * 65 / 100;
    let validation_end = n * 82 / 100;
    let trained_through = candles[train_end - 1].time;
    let version = format!("ridge-{}", candles[n - 1].time);
    let mut model = ForecastModel::fit(
        &samples(candles, train_end),
        0.1,
        version.clone(),
        trained_through,
    )
    .ok_or("Insufficient training samples")?;
    let all = samples(candles, n);
    let calibration: Vec<_> = all
        .into_iter()
        .filter(|s| s.index >= train_end && s.index + HORIZON < calibration_end)
        .collect();
    model.calibrate(&calibration);
    let baseline_policy = Policy::default();
    let mut best = baseline_policy.clone();
    let mut best_score = f64::NEG_INFINITY;
    let mut validation = Metrics::default();
    let mut seed = 0x43525950544F_u64;
    let trials = 64;
    for trial in 0..trials {
        let mut unit = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 11) as f64 / (1_u64 << 53) as f64
        };
        let policy = if trial == 0 {
            baseline_policy.clone()
        } else {
            Policy {
                min_edge: 0.001 + unit() * 0.006,
                uncertainty_weight: 0.25 + unit() * 1.25,
                take_profit_atr: 1.5 + unit() * 3.5,
                review_atr: 3.0,
                cooldown_bars: 4 + (unit() * 12.0) as usize,
                buy_size_bps: [2500, 5000, 7500, 10_000][(unit() * 4.0) as usize],
                sell_size_bps: [2500, 5000, 7500, 10_000][(unit() * 4.0) as usize],
                ..Policy::default()
            }
        };
        let middle = (calibration_end + validation_end) / 2;
        let left = backtest(candles, &model, &policy, calibration_end + HORIZON, middle);
        let right = backtest(candles, &model, &policy, middle + HORIZON, validation_end);
        let score = (left.net_return + right.net_return) / 2.0
            - 2.0 * left.max_drawdown.max(right.max_drawdown)
            - (left.net_return - right.net_return).abs();
        if score > best_score {
            best_score = score;
            best = policy;
            validation = backtest(
                candles,
                &model,
                &best,
                calibration_end + HORIZON,
                validation_end,
            );
        }
    }
    let holdout = backtest(candles, &model, &best, validation_end + HORIZON, n);
    let baseline = backtest(
        candles,
        &model,
        &baseline_policy,
        validation_end + HORIZON,
        n,
    );
    // This is an honest research candidate, not an automatically promoted live model.
    let status = if holdout.entries < 8 {
        "insufficient_evidence"
    } else if holdout.net_return <= 0.0 || holdout.max_drawdown >= 0.1 {
        "rejected"
    } else {
        "shadow_candidate"
    }
    .to_string();
    Ok(Research {execution_version:2,version,model,policy:best,trials,validation,holdout,baseline,status,trained_through,holdout_start:candles[validation_end+HORIZON].time,explanation:"Chronological 55% train / 10% calibration / 17% validation / 18% holdout. Four-bar gaps; 64 validation trials including buy/sell sizing. Fixed 20% reserves, two tradable layers. Simulated next-open fills with fees and slippage; live quotes must also cover network costs. Human review holds losing positions; no guaranteed stop. Research evidence only; no automatic promotion.".into()})
}

pub fn demo_candles() -> Vec<Candle> {
    (0..1000)
        .map(|i| {
            let t = i as f64;
            let price = 145.0 + 0.012 * t + 4.0 * (t / 41.0).sin() + 1.1 * (t / 7.0).cos();
            Candle {
                time: 1_793_000_000 + i * 900,
                open: price - 0.12,
                high: price + 0.65,
                low: price - 0.7,
                close: price,
                volume: 1000.0 + 200.0 * (t / 11.0).sin(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_block_does_not_change_selected_parameters() {
        let mut candles = demo_candles();
        let original = optimize(&candles).expect("research");
        for candle in candles.iter_mut().skip(850) {
            candle.close *= 0.5;
            candle.open *= 0.5;
            candle.high *= 0.5;
            candle.low *= 0.5;
        }
        let changed = optimize(&candles).expect("research");
        assert_eq!(original.policy.min_edge, changed.policy.min_edge);
        assert_eq!(original.policy.buy_size_bps, changed.policy.buy_size_bps);
        assert_eq!(original.policy.sell_size_bps, changed.policy.sell_size_bps);
        assert_eq!(original.model.weights, changed.model.weights);
        assert_eq!(original.model.means, changed.model.means);
    }
    #[test]
    fn future_labels_do_not_cross_training_boundary() {
        assert!(
            samples(&demo_candles(), 300)
                .iter()
                .all(|s| s.index + HORIZON < 300)
        );
    }
    #[test]
    fn rejects_duplicate_timestamps() {
        let mut candles = demo_candles();
        candles[500].time = candles[499].time;
        assert!(optimize(&candles).is_err());
    }
    #[test]
    fn a_later_crash_cannot_veto_an_earlier_fill_and_review_never_auto_sells() {
        let mut candles = demo_candles();
        let mut model = optimize(&candles).expect("model").model;
        model.weights = [0.10, 0.0, 0.0, 0.0, 0.0, 0.0];
        model.uncertainty = 0.0;
        for c in &mut candles {
            c.open = 10.0;
            c.close = 10.0;
            c.low = 9.9;
            c.high = 10.1;
        }
        candles[61].close = 1.0;
        candles[61].low = 1.0;
        candles[62].open = 1.0;
        candles[62].low = 1.0;
        candles[63].open = 20.0;
        candles[63].close = 20.0;
        candles[63].high = 20.1;
        candles[63].low = 19.9;
        let policy = Policy {
            cooldown_bars: 1,
            ..Policy::default()
        };
        let metrics = backtest(&candles, &model, &policy, 60, 65);
        assert_eq!(
            metrics.trades, 2,
            "Both planned opening buys execute; review cannot fabricate later sales"
        );
        assert!(metrics.review_triggered);
        assert!(metrics.max_drawdown > 0.20);
    }
}
