//! Real-money accounting, kept separate from simulated balances.
use serde::{Deserialize, Serialize};

pub const USDC: u64 = 1_000_000;
pub const SOL: u64 = 1_000_000_000;

pub fn decimal_units(text: &str, decimals: u32) -> Result<u64, String> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > decimals as usize
    {
        return Err("Enter a positive decimal without commas or exponent notation".into());
    }
    let scale = 10_u64.pow(decimals);
    let whole: u64 = whole.parse().map_err(|_| "Amount is too large")?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| "Invalid decimal")?
    };
    whole
        .checked_mul(scale)
        .and_then(|v| {
            v.checked_add(
                fraction
                    * 10_u64
                        .pow(decimals - text.split_once('.').map_or(0, |(_, f)| f.len()) as u32),
            )
        })
        .filter(|v| *v <= 1_000_000_000_000_000)
        .ok_or("Amount is too large".into())
}
pub fn ratio(value: u64, numerator: u64, denominator: u64) -> Result<u64, String> {
    if denominator == 0 {
        return Err("Cannot divide by zero".into());
    }
    u64::try_from(value as u128 * numerator as u128 / denominator as u128)
        .map_err(|_| "Amount overflow".into())
}
pub fn signed_difference(a: u64, b: u64) -> Result<i64, String> {
    i64::try_from(a as i128 - b as i128).map_err(|_| "Accounting overflow".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThbRate {
    /// THB per USDC, scaled by 1,000,000. Valuation, not a withdrawal quote.
    pub micros: u64,
    pub fetched_at: i64,
    pub source: String,
}
impl ThbRate {
    pub fn satang(&self, usdc: u64) -> Result<u64, String> {
        ratio(usdc, self.micros, 10_000_000_000)
    }
    pub fn fresh(&self, now: i64) -> bool {
        self.micros > 0 && (0..=600).contains(&(now - self.fetched_at))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveConfig {
    pub trading_budget: u64,
    pub max_buy: u64,
    pub reserve: u64,
    pub max_layers: usize,
    pub sol_gas_reserve: u64,
    pub max_network_fee: u64,
    pub slippage_bps: u16,
}
impl LiveConfig {
    pub fn validate(&self, funded: u64) -> Result<(), String> {
        if self.trading_budget == 0
            || self.max_buy == 0
            || self.max_buy > self.trading_budget
            || self.trading_budget > funded
            || self.reserve > funded - self.trading_budget
            || !(1..=4).contains(&self.max_layers)
            || !(1..=100).contains(&self.slippage_bps)
            || self.sol_gas_reserve < 5_000_000
            || self.max_network_fee == 0
            || self.max_network_fee > 2_000_000
        {
            return Err(
                "Check the funding, trade limits, layer count, slippage and SOL fee buffer".into(),
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealLayer {
    pub id: u64,
    pub quantity: u64,
    pub cost_usdc: u64,
    pub cost_satang: u64,
    pub entry_fee_usdc: u64,
    pub entry_fee_satang: u64,
    pub target_price: u64,
    pub review_price: u64,
    pub opened_at: i64,
    pub model: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealFill {
    pub signature: String,
    pub layer_id: u64,
    pub side: String,
    pub input: u64,
    pub output: u64,
    pub network_fee_lamports: u64,
    pub rent_delta_lamports: i64,
    pub realized_usdc: i64,
    pub realized_satang: i64,
    pub rate: ThbRate,
    pub time: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOrder {
    pub id: String,
    pub side: String,
    pub layer_id: u64,
    pub input: u64,
    pub expected_output: u64,
    pub minimum_output: u64,
    pub unsigned_transaction: String,
    pub request_id: String,
    pub last_valid_block_height: u64,
    pub prepared_at: i64,
    pub expires_at: i64,
    pub signature: Option<String>,
    pub status: String,
    pub network_fee: u64,
    pub rent_delta: i64,
    pub rate: ThbRate,
    pub price: u64,
    pub target_price: u64,
    pub review_price: u64,
    pub model: String,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealAccount {
    pub address: String,
    pub initial_usdc: u64,
    pub initial_cost_satang: u64,
    pub initial_reference_satang: u64,
    pub source_total_usdc: u64,
    pub source_total_cost_satang: u64,
    pub funding_fees_included: bool,
    pub cash: u64,
    pub cash_basis_satang: u64,
    pub config: LiveConfig,
    pub layers: Vec<RealLayer>,
    pub fills: Vec<RealFill>,
    pub pending: Option<PendingOrder>,
    pub realized_usdc: i64,
    pub realized_satang: i64,
    pub fees_usdc: u64,
    pub fees_satang: u64,
    pub parked_profit: u64,
    pub reconciliation_error: Option<String>,
    pub started_at: i64,
    pub advice_model: Option<crypto_bros_core::model::ForecastModel>,
    pub advice_policy: crypto_bros_core::model::Policy,
    pub last_advice_candle: i64,
}
impl RealAccount {
    pub fn new(
        address: String,
        allocated: u64,
        total_usdc: u64,
        total_satang: u64,
        fees_included: bool,
        config: LiveConfig,
        rate: &ThbRate,
        now: i64,
    ) -> Result<Self, String> {
        config.validate(allocated)?;
        if allocated == 0 || total_usdc < allocated || total_satang == 0 || !rate.fresh(now) {
            return Err(
                "Enter the received USDC and its original purchase cost, then refresh valuation"
                    .into(),
            );
        }
        let cost = u64::try_from(
            (total_satang as u128 * allocated as u128 + total_usdc as u128 / 2)
                / total_usdc as u128,
        )
        .map_err(|_| "Allocated cost overflow")?;
        Ok(Self {
            address,
            initial_usdc: allocated,
            initial_cost_satang: cost,
            initial_reference_satang: rate.satang(allocated)?,
            source_total_usdc: total_usdc,
            source_total_cost_satang: total_satang,
            funding_fees_included: fees_included,
            cash: allocated,
            cash_basis_satang: cost,
            config,
            layers: vec![],
            fills: vec![],
            pending: None,
            realized_usdc: 0,
            realized_satang: 0,
            fees_usdc: 0,
            fees_satang: 0,
            parked_profit: 0,
            reconciliation_error: None,
            started_at: now,
            advice_model: None,
            advice_policy: Default::default(),
            last_advice_candle: 0,
        })
    }
    pub fn equity(&self, price: u64) -> Result<u64, String> {
        self.layers.iter().try_fold(self.cash, |sum, l| {
            sum.checked_add(ratio(l.quantity, price, SOL)?)
                .ok_or("Equity overflow".into())
        })
    }
    pub fn entry_blocked(&self, price: u64) -> bool {
        let Ok(equity) = self.equity(price) else {
            return true;
        };
        self.reconciliation_error.is_some()
            || equity.saturating_sub(self.fees_usdc) <= self.initial_usdc.saturating_mul(90) / 100
            || self
                .layers
                .iter()
                .any(|l| l.quantity > 0 && price <= l.review_price)
    }
    pub fn check_buy(&self, amount: u64, price: u64) -> Result<(), String> {
        if self.pending.is_some() || self.entry_blocked(price) {
            return Err(
                "Resolve the pending order or risk review before opening another position".into(),
            );
        }
        let exposure = self.layers.iter().try_fold(0_u64, |a, l| {
            a.checked_add(l.cost_usdc).ok_or("Exposure overflow")
        })?;
        if amount == 0
            || amount > self.config.max_buy
            || exposure.saturating_add(amount) > self.config.trading_budget
            || self.layers.iter().filter(|l| l.quantity > 0).count() >= self.config.max_layers
            || self.cash.saturating_sub(amount)
                < self.config.reserve.saturating_add(self.parked_profit)
            || amount > self.cash
        {
            return Err("This buy exceeds the entry, exposure, layer or reserve limit".into());
        }
        Ok(())
    }
    pub fn apply_fill(
        &mut self,
        order: &PendingOrder,
        signature: &str,
        input: u64,
        output: u64,
        network_fee: u64,
        rent_delta: i64,
        time: i64,
    ) -> Result<(), String> {
        if self.fills.iter().any(|f| f.signature == signature) {
            return Ok(());
        }
        if input != order.input
            || output < order.minimum_output
            || network_fee > self.config.max_network_fee
        {
            return Err(
                "Confirmed amounts differ from the approved limits; reconciliation required".into(),
            );
        }
        // Apply to a clone so failed accounting never leaves a partially booked trade.
        let mut next = self.clone();
        let fee_usdc = ratio(network_fee, order.price, SOL)?;
        let fee_satang = order.rate.satang(fee_usdc)?;
        let (layer_id, gain_usdc, gain_thb) = if order.side == "buy" {
            if input > next.cash || output == 0 {
                return Err("Insufficient tracked cash".into());
            }
            let disposed_basis = ratio(next.cash_basis_satang, input, next.cash)?;
            let acquisition = order.rate.satang(input)?;
            next.cash -= input;
            next.cash_basis_satang -= disposed_basis;
            let id = next.layers.last().map_or(1, |l| l.id + 1);
            let execution_price = ratio(input, SOL, output)?;
            next.layers.push(RealLayer {
                id,
                quantity: output,
                cost_usdc: input,
                cost_satang: acquisition,
                entry_fee_usdc: fee_usdc,
                entry_fee_satang: fee_satang,
                target_price: execution_price
                    .saturating_add(order.target_price.saturating_sub(order.price)),
                review_price: execution_price
                    .saturating_sub(order.price.saturating_sub(order.review_price))
                    .max(1),
                opened_at: time,
                model: order.model.clone(),
            });
            (id, 0, signed_difference(acquisition, disposed_basis)?)
        } else if order.side == "sell" {
            let layer = next
                .layers
                .iter_mut()
                .find(|l| l.id == order.layer_id)
                .ok_or("Tracked layer is missing")?;
            if input == 0 || input > layer.quantity {
                return Err("Sale exceeds tracked SOL".into());
            }
            let cost = ratio(layer.cost_usdc, input, layer.quantity)?;
            let cost_thb = ratio(layer.cost_satang, input, layer.quantity)?;
            let entry_fee = ratio(layer.entry_fee_usdc, input, layer.quantity)?;
            let entry_fee_thb = ratio(layer.entry_fee_satang, input, layer.quantity)?;
            layer.quantity -= input;
            layer.cost_usdc -= cost;
            layer.cost_satang -= cost_thb;
            layer.entry_fee_usdc -= entry_fee;
            layer.entry_fee_satang -= entry_fee_thb;
            next.cash = next.cash.checked_add(output).ok_or("Cash overflow")?;
            let proceeds_thb = order.rate.satang(output)?;
            next.cash_basis_satang = next
                .cash_basis_satang
                .checked_add(proceeds_thb)
                .ok_or("Cost overflow")?;
            let gain = signed_difference(output, cost)?;
            next.parked_profit = next
                .parked_profit
                .checked_add(
                    output
                        .saturating_sub(cost)
                        .saturating_sub(fee_usdc)
                        .saturating_sub(entry_fee),
                )
                .ok_or("Profit overflow")?;
            (
                order.layer_id,
                gain,
                signed_difference(proceeds_thb, cost_thb)?,
            )
        } else {
            return Err("Unknown order direction".into());
        };
        next.realized_usdc = next
            .realized_usdc
            .checked_add(gain_usdc)
            .ok_or("Profit overflow")?;
        next.realized_satang = next
            .realized_satang
            .checked_add(gain_thb)
            .ok_or("Profit overflow")?;
        next.fees_usdc = next.fees_usdc.checked_add(fee_usdc).ok_or("Fee overflow")?;
        next.fees_satang = next
            .fees_satang
            .checked_add(fee_satang)
            .ok_or("Fee overflow")?;
        next.fills.push(RealFill {
            signature: signature.into(),
            layer_id,
            side: order.side.clone(),
            input,
            output,
            network_fee_lamports: network_fee,
            rent_delta_lamports: rent_delta,
            realized_usdc: gain_usdc,
            realized_satang: gain_thb,
            rate: order.rate.clone(),
            time,
        });
        next.pending = None;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimal_and_partial_funding_cost() {
        assert_eq!(decimal_units("20.1", 6).unwrap(), 20_100_000);
        assert_eq!(decimal_units("0.000000001", 9).unwrap(), 1);
        for bad in ["-1", "1e3", "1.0000001", "NaN", "1,000"] {
            assert!(decimal_units(bad, 6).is_err());
        }
        assert_eq!(ratio(100_000, 25_000_000, 100_000_000).unwrap(), 25_000);
    }
    fn account() -> RealAccount {
        let rate = ThbRate {
            micros: 35_000_000,
            fetched_at: 10,
            source: "test".into(),
        };
        RealAccount::new(
            "public-test-address".into(),
            100 * USDC,
            100 * USDC,
            360_000,
            true,
            LiveConfig {
                trading_budget: 20 * USDC,
                max_buy: 10 * USDC,
                reserve: 80 * USDC,
                max_layers: 2,
                sol_gas_reserve: 20_000_000,
                max_network_fee: 1_000_000,
                slippage_bps: 50,
            },
            &rate,
            10,
        )
        .unwrap()
    }
    #[test]
    fn real_limits_cannot_spend_reserve_or_change_with_a_model() {
        let mut a = account();
        assert!(a.check_buy(10 * USDC, 100 * USDC).is_ok());
        assert!(a.check_buy(11 * USDC, 100 * USDC).is_err());
        a.cash = 89 * USDC;
        assert!(a.check_buy(10 * USDC, 100 * USDC).is_err());
    }
    #[test]
    fn actual_fills_are_idempotent_and_partial_sales_keep_basis() {
        let mut a = account();
        let mut o = PendingOrder {
            id: "test".into(),
            side: "buy".into(),
            layer_id: 0,
            input: 10 * USDC,
            expected_output: SOL / 10,
            minimum_output: SOL / 11,
            unsigned_transaction: String::new(),
            request_id: String::new(),
            last_valid_block_height: 1,
            prepared_at: 10,
            expires_at: 60,
            signature: None,
            status: "test".into(),
            network_fee: 5000,
            rent_delta: 0,
            rate: ThbRate {
                micros: 35_000_000,
                fetched_at: 10,
                source: "test".into(),
            },
            price: 100 * USDC,
            target_price: 110 * USDC,
            review_price: 90 * USDC,
            model: "test".into(),
            message: None,
        };
        a.apply_fill(&o, "signature-one", 10 * USDC, SOL / 10, 5000, 0, 20)
            .unwrap();
        a.apply_fill(&o, "signature-one", 10 * USDC, SOL / 10, 5000, 0, 20)
            .unwrap();
        assert_eq!(a.fills.len(), 1);
        assert_eq!(a.realized_satang, -1000); // realized FX against the original baht purchase cost
        o.side = "sell".into();
        o.layer_id = 1;
        o.input = SOL / 20;
        o.minimum_output = 5 * USDC;
        o.price = 110 * USDC;
        a.apply_fill(&o, "signature-two", SOL / 20, 5_500_000, 5000, 0, 30)
            .unwrap();
        assert_eq!(a.layers[0].quantity, SOL / 20);
        assert_eq!(a.layers[0].cost_usdc, 5 * USDC);
        assert_eq!(a.layers[0].cost_satang, 17_500);
        assert_eq!(a.realized_usdc, 500_000);
        let before = a.cash;
        assert!(
            a.apply_fill(&o, "invalid", SOL / 20, 4 * USDC, 5000, 0, 40)
                .is_err()
        );
        assert_eq!(a.cash, before);
    }
}
