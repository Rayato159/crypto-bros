use crate::indicators::{Candle, calculate};
use serde::{Deserialize, Serialize};

pub const HORIZON: usize = 4;

#[derive(Debug, Clone)]
pub struct Sample {
    pub index: usize,
    pub x: [f64; 5],
    pub y: f64,
}

pub fn samples(candles: &[Candle], label_end: usize) -> Vec<Sample> {
    let features = calculate(candles);
    let end = label_end.min(candles.len());
    (49..end.saturating_sub(HORIZON))
        .filter_map(|i| {
            let x = features[i].as_ref()?.features;
            Some(Sample {
                index: i,
                x,
                y: candles[i + HORIZON].close / candles[i].close - 1.0,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastModel {
    pub version: String,
    pub weights: [f64; 6],
    pub means: [f64; 5],
    pub scales: [f64; 5],
    pub uncertainty: f64,
    pub trained_samples: usize,
    pub training_end: i64,
}

impl ForecastModel {
    /// Fit scaler and ridge weights on training samples only; pivoted elimination
    /// on a small regularized system. No future label may cross training_end.
    pub fn fit(data: &[Sample], ridge: f64, version: String, training_end: i64) -> Option<Self> {
        if data.len() < 80 || !ridge.is_finite() || ridge <= 0.0 {
            return None;
        }
        let mut means = [0.0; 5];
        let mut scales = [0.0; 5];
        for s in data {
            for (j, mean) in means.iter_mut().enumerate() {
                *mean += s.x[j] / data.len() as f64;
            }
        }
        for s in data {
            for (j, scale) in scales.iter_mut().enumerate() {
                *scale += (s.x[j] - means[j]).powi(2) / data.len() as f64;
            }
        }
        for s in &mut scales {
            *s = s.sqrt().max(1e-8);
        }
        let mut a = [[0.0; 7]; 6];
        for s in data {
            let mut x = [1.0; 6];
            for j in 0..5 {
                x[j + 1] = (s.x[j] - means[j]) / scales[j];
            }
            for j in 0..6 {
                for k in 0..6 {
                    a[j][k] += x[j] * x[k];
                }
                a[j][6] += x[j] * s.y;
            }
        }
        for (j, row) in a.iter_mut().enumerate().skip(1) {
            row[j] += ridge * data.len() as f64;
        }
        for col in 0..6 {
            let pivot = (col..6)
                .max_by(|&a_idx, &b_idx| a[a_idx][col].abs().total_cmp(&a[b_idx][col].abs()))?;
            a.swap(col, pivot);
            let divisor = a[col][col];
            if !divisor.is_finite() || divisor.abs() < 1e-12 {
                return None;
            }
            for k in col..7 {
                a[col][k] /= divisor;
            }
            let pivot_row = a[col];
            for (row_idx, row) in a.iter_mut().enumerate() {
                if row_idx == col {
                    continue;
                }
                let factor = row[col];
                for k in col..7 {
                    row[k] -= factor * pivot_row[k];
                }
            }
        }
        let mut weights = [0.0; 6];
        for j in 0..6 {
            weights[j] = a[j][6];
        }
        if !weights.iter().all(|w| w.is_finite()) {
            return None;
        }
        Some(Self {
            version,
            weights,
            means,
            scales,
            uncertainty: 0.02,
            trained_samples: data.len(),
            training_end,
        })
    }
    pub fn predict(&self, x: [f64; 5]) -> f64 {
        self.weights[0]
            + (0..5)
                .map(|j| self.weights[j + 1] * (x[j] - self.means[j]) / self.scales[j])
                .sum::<f64>()
    }
    /// Calibration uses a later block not used for fitting weights or scaling.
    pub fn calibrate(&mut self, data: &[Sample]) {
        if data.is_empty() {
            return;
        }
        let mse = data
            .iter()
            .map(|s| (self.predict(s.x) - s.y).powi(2))
            .sum::<f64>()
            / data.len() as f64;
        self.uncertainty = mse.sqrt().max(0.001);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub min_edge: f64,
    pub uncertainty_weight: f64,
    pub take_profit_atr: f64,
    pub review_atr: f64,
    pub cooldown_bars: usize,
    pub fee_bps: u64,
    pub slippage_bps: u64,
    /// Fractions of the bounded per-slot buy amount / tradable layer quantity.
    #[serde(default = "full_allocation")]
    pub buy_size_bps: u16,
    #[serde(default = "full_allocation")]
    pub sell_size_bps: u16,
}
fn full_allocation() -> u16 {
    10_000
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            min_edge: 0.002,
            uncertainty_weight: 0.75,
            take_profit_atr: 2.5,
            review_atr: 3.0,
            cooldown_bars: 4,
            fee_bps: 20,
            slippage_bps: 10,
            buy_size_bps: 10_000,
            sell_size_bps: 10_000,
        }
    }
}
impl Policy {
    pub fn buy_size(&self, bounded_amount: u64) -> u64 {
        (bounded_amount as u128 * self.buy_size_bps.min(10_000) as u128 / 10_000) as u64
    }
    pub fn sell_size(&self, tradable_quantity: u64) -> u64 {
        (tradable_quantity as u128 * self.sell_size_bps.min(10_000) as u128 / 10_000) as u64
    }
    pub fn net_edge(&self, prediction: f64, uncertainty: f64) -> f64 {
        prediction
            - self.uncertainty_weight * uncertainty
            - 2.0 * (self.fee_bps + self.slippage_bps) as f64 / 10_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ridge_learns_relationship_without_lookahead() {
        let data: Vec<_> = (0..200)
            .map(|i| {
                let x = i as f64 / 200.0;
                Sample {
                    index: i,
                    x: [x, x * x, x.sin(), 0.0, 1.0],
                    y: 0.002 + 0.01 * x,
                }
            })
            .collect();
        let model = ForecastModel::fit(&data, 0.001, "test".into(), 0).expect("fit");
        assert!((model.predict([0.5, 0.25, 0.5_f64.sin(), 0.0, 1.0]) - 0.007).abs() < 0.001);
        assert!(model.weights.iter().all(|w| w.is_finite()));
    }
    #[test]
    fn costs_can_veto_an_upward_forecast() {
        assert!(Policy::default().net_edge(0.005, 0.002) < 0.0);
    }
}
