use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    pub fn valid(&self) -> bool {
        [self.open, self.high, self.low, self.close, self.volume]
            .iter()
            .all(|v| v.is_finite())
            && self.low > 0.0
            && self.volume >= 0.0
            && self.high >= self.open.max(self.close)
            && self.low <= self.open.min(self.close)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Indicators {
    pub rsi: f64,
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
    pub atr: f64,
    pub features: [f64; 5],
}

/// A causal series: output i depends only on candles [0..=i]. Wilder RSI/ATR,
/// EMA(12,26), signal EMA(9); require 50 completed candles for warmup.
pub fn calculate(candles: &[Candle]) -> Vec<Option<Indicators>> {
    let mut out = Vec::with_capacity(candles.len());
    let Some(first) = candles.first() else {
        return out;
    };
    let (mut fast, mut slow, mut signal) = (first.close, first.close, 0.0);
    let (mut gain, mut loss, mut atr) = (0.0, 0.0, 0.0);
    for (i, c) in candles.iter().enumerate() {
        fast += 2.0 / 13.0 * (c.close - fast);
        slow += 2.0 / 27.0 * (c.close - slow);
        let macd = fast - slow;
        signal += 2.0 / 10.0 * (macd - signal);
        if i > 0 {
            let prev = candles[i - 1].close;
            let delta = c.close - prev;
            let tr = (c.high - c.low)
                .max((c.high - prev).abs())
                .max((c.low - prev).abs());
            if i <= 14 {
                gain += delta.max(0.0) / 14.0;
                loss += (-delta).max(0.0) / 14.0;
                atr += tr / 14.0;
            } else {
                gain = (gain * 13.0 + delta.max(0.0)) / 14.0;
                loss = (loss * 13.0 + (-delta).max(0.0)) / 14.0;
                atr = (atr * 13.0 + tr) / 14.0;
            }
        }
        if i < 49 {
            out.push(None);
            continue;
        }
        let rsi = if gain + loss < 1e-14 {
            50.0
        } else {
            100.0 * gain / (gain + loss)
        };
        let volume_mean = candles[i - 19..=i].iter().map(|x| x.volume).sum::<f64>() / 20.0;
        let volume_ratio = if volume_mean > 0.0 {
            (c.volume / volume_mean).ln_1p()
        } else {
            0.0
        };
        out.push(Some(Indicators {
            rsi,
            macd,
            signal,
            histogram: macd - signal,
            atr,
            features: [
                (rsi - 50.0) / 50.0,
                (macd - signal) / c.close,
                c.close / candles[i - 4].close - 1.0,
                atr / c.close,
                volume_ratio,
            ],
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn future_candles_cannot_change_past_indicators() {
        let mut data: Vec<_> = (0..90)
            .map(|i| Candle {
                time: i,
                open: 100.0 + i as f64,
                high: 102.0 + i as f64,
                low: 99.0 + i as f64,
                close: 101.0 + i as f64,
                volume: 10.0,
            })
            .collect();
        let before = calculate(&data)[60].clone().expect("warmup");
        data[80].close = 9999.0;
        let after = calculate(&data)[60].clone().expect("warmup");
        assert_eq!(before.features, after.features);
        assert!(before.rsi > 99.0);
    }
}
