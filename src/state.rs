use crypto_bros_core::{
    indicators::{Candle, Indicators, calculate},
    ledger::{Ledger, USDC, price_units},
    model::{ForecastModel, Policy},
    research::Research,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub decision_id: u64,
    pub rating: String,
    pub note: String,
    pub time: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: u64,
    pub time: i64,
    pub action: String,
    pub reason: String,
    pub model: String,
    pub forecast: f64,
    pub net_edge: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: u64,
    pub time: i64,
    pub message: String,
    pub acknowledged: bool,
    pub email_status: String,
    pub attempts: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub amount_usdc: f64,
    pub sol_out: f64,
    pub effective_price: f64,
    pub fee_bps: u64,
    pub time: i64,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub address: String,
    pub sol: f64,
    pub usdc: f64,
    pub verified_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema: u32,
    pub app_version: String,
    pub mode: String,
    pub running: bool,
    pub source: String,
    pub market_error: Option<String>,
    pub updated_at: i64,
    pub price: f64,
    pub candles: Vec<Candle>,
    pub indicators: Option<Indicators>,
    pub ledger: Ledger,
    pub research: Option<Research>,
    pub active_model: Option<ForecastModel>,
    pub policy: Policy,
    pub decisions: Vec<Decision>,
    pub feedback: Vec<Feedback>,
    pub alerts: Vec<Alert>,
    pub wallet: Option<Wallet>,
    pub quote: Option<Quote>,
    pub last_processed_candle: i64,
    pub last_entry_candle: i64,
    pub research_end: i64,
    pub next_research_at: i64,
    pub optimizing: bool,
    pub equity: f64,
    pub net_return: f64,
    pub drawdown: f64,
    pub weekly_target: f64,
    pub email_configured: bool,
    pub jupiter_key_configured: bool,
    pub live_enabled: bool,
    #[serde(default)]
    pub real: Option<crate::live::RealAccount>,
    #[serde(default)]
    pub thb_rate: Option<crate::live::ThbRate>,
    #[serde(default)]
    pub real_balances: Option<crate::chain::Balances>,
    #[serde(default)]
    pub updating: bool,
    #[serde(default)]
    pub watch: crate::watch::Watch,
}
impl Snapshot {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            schema: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            mode: "paper".into(),
            running: false,
            source: "not_loaded".into(),
            market_error: None,
            updated_at: 0,
            price: 0.0,
            candles: vec![],
            indicators: None,
            ledger: Ledger::new(1000 * USDC, 400 * USDC, 4).map_err(|e| e.to_string())?,
            research: None,
            active_model: None,
            policy: Policy::default(),
            decisions: vec![],
            feedback: vec![],
            alerts: vec![],
            wallet: None,
            quote: None,
            last_processed_candle: 0,
            last_entry_candle: 0,
            research_end: 0,
            next_research_at: 0,
            optimizing: false,
            equity: 1000.0,
            net_return: 0.0,
            drawdown: 0.0,
            weekly_target: 0.05,
            email_configured: false,
            jupiter_key_configured: false,
            live_enabled: false,
            real: None,
            thb_rate: None,
            real_balances: None,
            updating: false,
            watch: Default::default(),
        })
    }
    pub fn revalue(&mut self) -> Result<(), String> {
        if self.price <= 0.0 {
            return Ok(());
        }
        let price = price_units(self.price).map_err(|e| e.to_string())?;
        self.drawdown = self.ledger.mark(price).map_err(|e| e.to_string())?;
        self.equity = self.ledger.equity(price).map_err(|e| e.to_string())? as f64 / USDC as f64;
        self.net_return = self.equity / (self.ledger.starting_equity as f64 / USDC as f64) - 1.0;
        self.indicators = calculate(&self.candles).last().cloned().flatten();
        Ok(())
    }
    pub fn decision(&mut self, action: &str, reason: String, forecast: f64, edge: f64) {
        let id = self.decisions.last().map_or(1, |d| d.id + 1);
        self.decisions.push(Decision {
            id,
            time: chrono::Utc::now().timestamp(),
            action: action.into(),
            reason,
            model: self
                .active_model
                .as_ref()
                .map_or("manual".into(), |m| m.version.clone()),
            forecast,
            net_edge: edge,
        });
        if self.decisions.len() > 500 {
            self.decisions.remove(0);
        }
    }
    pub fn alert(&mut self, message: String) {
        if self
            .alerts
            .iter()
            .any(|a| !a.acknowledged && a.message == message)
        {
            return;
        }
        let id = self.alerts.last().map_or(1, |a| a.id + 1);
        self.alerts.push(Alert {
            id,
            time: chrono::Utc::now().timestamp(),
            message,
            acknowledged: false,
            email_status: if self.email_configured {
                "queued"
            } else {
                "not_configured"
            }
            .into(),
            attempts: 0,
        });
    }
    pub fn fresh(&self) -> bool {
        self.price.is_finite()
            && self.price > 0.0
            && (self.source == "synthetic_demo"
                || (0..120).contains(&(chrono::Utc::now().timestamp() - self.updated_at)))
    }
    pub fn risk_blocked(&self) -> bool {
        self.drawdown >= 0.10
            || self.net_return <= -0.50
            || self
                .ledger
                .layers
                .iter()
                .any(|l| l.quantity > 0 && self.price <= l.review_price as f64 / USDC as f64)
    }
    pub fn evaluate(&mut self) -> Result<(), String> {
        self.revalue()?;
        if self.risk_blocked() {
            self.running = false;
            self.alert("Risk review required. New entries paused; positions remain open until your decision. No automatic stop-loss was executed.".into());
            return Ok(());
        }
        if !self.running || !self.fresh() || self.updating {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        let sell_ids: Vec<_> = self
            .ledger
            .layers
            .iter()
            .filter(|l| l.quantity > 0 && self.price >= l.target_price as f64 / USDC as f64)
            .map(|l| (l.id, l.quantity))
            .collect();
        for (id, qty) in sell_ids {
            let price =
                price_units(self.price * (1.0 - self.policy.slippage_bps as f64 / 10_000.0))
                    .map_err(|e| e.to_string())?;
            self.ledger
                .sell(id, qty, price, self.policy.fee_bps, now)
                .map_err(|e| e.to_string())?;
            self.decision(
                "sell",
                format!("Paper take-profit for layer {id}; fees and slippage deducted."),
                0.0,
                0.0,
            );
        }
        let Some(candle) = self.candles.last() else {
            return Ok(());
        };
        let candle_time = candle.time;
        if candle_time <= self.last_processed_candle {
            return self.revalue();
        }
        self.last_processed_candle = candle_time;
        let (Some(model), Some(ind)) = (&self.active_model, &self.indicators) else {
            return Ok(());
        };
        let prediction = model.predict(ind.features);
        let edge = self.policy.net_edge(prediction, model.uncertainty);
        if self.ledger.open_count() >= self.ledger.max_layers {
            self.decision(
                "wait",
                "All layer slots are occupied.".into(),
                prediction,
                edge,
            );
        } else if candle_time - self.last_entry_candle < self.policy.cooldown_bars as i64 * 900 {
            self.decision("wait", "Layer cooldown is active.".into(), prediction, edge);
        } else if edge > self.policy.min_edge {
            self.paper_buy()?;
            self.last_entry_candle = candle_time;
            self.decision(
                "buy",
                "Forecast cleared uncertainty, simulated costs and minimum edge. Paper fill only."
                    .into(),
                prediction,
                edge,
            );
        } else {
            self.decision(
                "wait",
                "Forecast does not clear uncertainty and round-trip costs.".into(),
                prediction,
                edge,
            );
        }
        self.revalue()
    }
    pub fn paper_buy(&mut self) -> Result<(), String> {
        self.revalue()?;
        if self.updating || self.risk_blocked() {
            return Err("New entries are blocked by a hard risk limit or update. Acknowledging an alert does not override this limit.".into());
        }
        if !self.fresh() {
            return Err("Market data is missing or stale".into());
        }
        if self.alerts.iter().any(|a| !a.acknowledged) {
            return Err("Acknowledge risk review before opening a new paper layer".into());
        }
        let ind = self
            .indicators
            .as_ref()
            .ok_or("Indicators need more completed candles")?;
        let fill_price = self.price * (1.0 + self.policy.slippage_bps as f64 / 10_000.0);
        let p = price_units(fill_price).map_err(|e| e.to_string())?;
        let target = price_units(fill_price + self.policy.take_profit_atr * ind.atr)
            .map_err(|e| e.to_string())?;
        let review = price_units((fill_price - self.policy.review_atr * ind.atr).max(0.01))
            .map_err(|e| e.to_string())?;
        let budget =
            (self.ledger.starting_equity - self.ledger.reserve) / self.ledger.max_layers as u64;
        let model = self
            .active_model
            .as_ref()
            .map_or("manual-paper", |m| m.version.as_str());
        self.ledger
            .buy(
                budget,
                p,
                self.policy.fee_bps,
                chrono::Utc::now().timestamp(),
                model,
                target,
                review,
            )
            .map_err(|e| e.to_string())?;
        self.revalue()
    }
}
