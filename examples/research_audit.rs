//! Read-only diagnostics for public Binance kline JSON. No service, wallet or env access.
use crypto_bros_core::{
    indicators::{Candle, calculate},
    research::optimize,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Pass a public kline JSON file")?;
    let rows: Vec<Vec<Value>> = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut candles = Vec::new();
    for row in rows {
        let value = |index: usize| -> Result<f64, Box<dyn std::error::Error>> {
            Ok(row
                .get(index)
                .and_then(Value::as_str)
                .ok_or("Missing number")?
                .parse()?)
        };
        candles.push(Candle {
            time: row.first().and_then(Value::as_i64).ok_or("Missing time")? / 1000,
            open: value(1)?,
            high: value(2)?,
            low: value(3)?,
            close: value(4)?,
            volume: value(5)?,
        });
    }
    // The provider's live 1000-row response usually contains one unclosed bar.
    for count in [candles.len(), candles.len().saturating_sub(1)] {
        let input = &candles[candles.len() - count..];
        let result = optimize(input)?;
        let features = calculate(input);
        let start = count * 82 / 100 + 4;
        let predictions: Vec<_> = features[start - 1..count - 1]
            .iter()
            .flatten()
            .map(|i| result.model.predict(i.features))
            .collect();
        let threshold = result.policy.min_edge
            + result.policy.uncertainty_weight * result.model.uncertainty
            + 2.0 * (result.policy.fee_bps + result.policy.slippage_bps) as f64 / 10_000.0;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "candles": count, "version": result.version, "status": result.status,
                "entries": result.holdout.entries, "fills": result.holdout.trades,
                "return": result.holdout.net_return, "drawdown": result.holdout.max_drawdown,
                "forecast_max": predictions.iter().copied().reduce(f64::max),
                "forecast_min": predictions.iter().copied().reduce(f64::min),
                "forecast_required": threshold,
                "signals_passing_edge": predictions.iter().filter(|p| **p > threshold).count(),
                "uncertainty": result.model.uncertainty, "policy": result.policy,
            }))?
        );
    }
    Ok(())
}
