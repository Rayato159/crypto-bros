use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const USDC: u64 = 1_000_000;
pub const SOL: u64 = 1_000_000_000;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("Amount or price is invalid")]
    Invalid,
    #[error("Insufficient deployable cash; reserve and parked profit are protected")]
    Reserve,
    #[error("Maximum number of open layers reached")]
    Layers,
    #[error("Layer or sell quantity is invalid")]
    Position,
    #[error("Arithmetic limit exceeded")]
    Overflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: u64,
    pub opened_at: i64,
    pub quantity: u64,
    #[serde(default)]
    pub reserved_quantity: u64,
    pub remaining_cost: u64,
    pub entry_price: u64,
    pub target_price: u64,
    pub review_price: u64,
    pub realized_pnl: i64,
    pub model_version: String,
}
impl Layer {
    pub fn tradable_quantity(&self) -> u64 {
        self.quantity.saturating_sub(self.reserved_quantity)
    }
}
pub fn reserved_part(amount: u64, bps: u16) -> u64 {
    (amount as u128 * bps.min(10_000) as u128).div_ceil(10_000) as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub id: u64,
    pub time: i64,
    pub side: String,
    pub layer_id: u64,
    pub quantity: u64,
    pub price: u64,
    pub fee: u64,
    pub cash_flow: i64,
    pub pnl: i64,
    pub source: String,
    pub model_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub starting_equity: u64,
    pub cash: u64,
    pub reserve: u64,
    pub parked_profit: u64,
    pub high_water: u64,
    pub max_layers: usize,
    pub layers: Vec<Layer>,
    pub fills: Vec<Fill>,
    pub next_layer_id: u64,
    #[serde(default)]
    pub reserve_bps: u16,
}

fn units(value: u128) -> Result<u64, LedgerError> {
    u64::try_from(value).map_err(|_| LedgerError::Overflow)
}
fn fee(gross: u64, bps: u64) -> Result<u64, LedgerError> {
    units((gross as u128 * bps as u128).div_ceil(10_000))
}
pub fn price_units(price: f64) -> Result<u64, LedgerError> {
    if !price.is_finite() || price <= 0.0 || price > 1_000_000.0 {
        return Err(LedgerError::Invalid);
    }
    Ok((price * USDC as f64).round() as u64)
}

impl Ledger {
    pub fn new(capital: u64, reserve: u64, max_layers: usize) -> Result<Self, LedgerError> {
        if capital == 0
            || capital > i64::MAX as u64 / 4
            || reserve >= capital
            || !(1..=20).contains(&max_layers)
        {
            return Err(LedgerError::Invalid);
        }
        Ok(Self {
            starting_equity: capital,
            cash: capital,
            reserve,
            parked_profit: 0,
            high_water: capital,
            max_layers,
            layers: vec![],
            fills: vec![],
            next_layer_id: 1,
            reserve_bps: 0,
        })
    }
    pub fn available(&self) -> u64 {
        self.cash
            .saturating_sub(self.reserve)
            .saturating_sub(self.parked_profit)
    }
    pub fn open_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|l| l.tradable_quantity() > 0)
            .count()
    }
    pub fn equity(&self, price: u64) -> Result<u64, LedgerError> {
        units(
            self.cash as u128
                + self
                    .layers
                    .iter()
                    .map(|l| l.quantity as u128 * price as u128 / SOL as u128)
                    .sum::<u128>(),
        )
    }
    pub fn mark(&mut self, price: u64) -> Result<f64, LedgerError> {
        let equity = self.equity(price)?;
        self.high_water = self.high_water.max(equity);
        Ok(1.0 - equity as f64 / self.high_water as f64)
    }
    pub fn buy(
        &mut self,
        budget: u64,
        price: u64,
        fee_bps: u64,
        time: i64,
        model: &str,
        target: u64,
        review: u64,
    ) -> Result<Fill, LedgerError> {
        if budget == 0 || price == 0 || fee_bps > 1000 || target <= price || review >= price {
            return Err(LedgerError::Invalid);
        }
        if budget > self.available() {
            return Err(LedgerError::Reserve);
        }
        if self.open_count() >= self.max_layers {
            return Err(LedgerError::Layers);
        }
        let principal = units(budget as u128 * 10_000 / (10_000 + fee_bps) as u128)?;
        let quantity = units(principal as u128 * SOL as u128 / price as u128)?;
        if quantity == 0 {
            return Err(LedgerError::Invalid);
        }
        let gross = units((quantity as u128 * price as u128).div_ceil(SOL as u128))?;
        let charge = fee(gross, fee_bps)?;
        let cost = gross.checked_add(charge).ok_or(LedgerError::Overflow)?;
        if cost > budget || cost > self.available() {
            return Err(LedgerError::Reserve);
        }
        let id = self.next_layer_id;
        let fill = Fill {
            id: self.fills.len() as u64 + 1,
            time,
            side: "buy".into(),
            layer_id: id,
            quantity,
            price,
            fee: charge,
            cash_flow: -(cost as i64),
            pnl: 0,
            source: "paper".into(),
            model_version: model.into(),
        };
        self.cash -= cost;
        self.next_layer_id += 1;
        self.layers.push(Layer {
            id,
            opened_at: time,
            quantity,
            reserved_quantity: reserved_part(quantity, self.reserve_bps),
            remaining_cost: cost,
            entry_price: price,
            target_price: target,
            review_price: review,
            realized_pnl: 0,
            model_version: model.into(),
        });
        self.fills.push(fill.clone());
        Ok(fill)
    }
    pub fn sell(
        &mut self,
        id: u64,
        quantity: u64,
        price: u64,
        fee_bps: u64,
        time: i64,
    ) -> Result<Fill, LedgerError> {
        if price == 0 || fee_bps > 1000 {
            return Err(LedgerError::Invalid);
        }
        let layer = self
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or(LedgerError::Position)?;
        if quantity == 0 || quantity > layer.tradable_quantity() {
            return Err(LedgerError::Position);
        }
        let gross = units(quantity as u128 * price as u128 / SOL as u128)?;
        let charge = fee(gross, fee_bps)?.min(gross);
        let proceeds = gross - charge;
        let basis = if quantity == layer.quantity {
            layer.remaining_cost
        } else {
            units(layer.remaining_cost as u128 * quantity as u128 / layer.quantity as u128)?
        };
        let pnl = i64::try_from(proceeds).map_err(|_| LedgerError::Overflow)?
            - i64::try_from(basis).map_err(|_| LedgerError::Overflow)?;
        let new_cash = self
            .cash
            .checked_add(proceeds)
            .ok_or(LedgerError::Overflow)?;
        let reserved_proceeds = reserved_part(proceeds, self.reserve_bps);
        let new_reserve = self
            .reserve
            .checked_add(reserved_proceeds)
            .ok_or(LedgerError::Overflow)?;
        let profit = pnl.max(0) as u64;
        let parked = self
            .parked_profit
            .checked_add(profit.saturating_sub(reserved_part(profit, self.reserve_bps)))
            .ok_or(LedgerError::Overflow)?;
        let fill = Fill {
            id: self.fills.len() as u64 + 1,
            time,
            side: "sell".into(),
            layer_id: id,
            quantity,
            price,
            fee: charge,
            cash_flow: proceeds as i64,
            pnl,
            source: "paper".into(),
            model_version: layer.model_version.clone(),
        };
        layer.quantity -= quantity;
        layer.remaining_cost -= basis;
        layer.realized_pnl += pnl;
        self.cash = new_cash;
        self.reserve = new_reserve;
        self.parked_profit = parked;
        self.fills.push(fill.clone());
        Ok(fill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserve_and_layer_caps_survive_repeated_orders() {
        let mut l = Ledger::new(1000 * USDC, 400 * USDC, 4).expect("valid");
        for _ in 0..4 {
            l.buy(
                150 * USDC,
                150 * USDC,
                20,
                0,
                "test",
                160 * USDC,
                140 * USDC,
            )
            .expect("buy");
        }
        assert!(l.cash >= l.reserve);
        assert!(
            l.buy(1, 150 * USDC, 20, 0, "test", 160 * USDC, 140 * USDC)
                .is_err()
        );
        assert_eq!(l.open_count(), 4);
    }
    #[test]
    fn partial_sales_preserve_cost_and_park_realized_profit() {
        let mut l = Ledger::new(1000 * USDC, 400 * USDC, 4).expect("valid");
        let buy = l
            .buy(150 * USDC, 100 * USDC, 20, 0, "v1", 120 * USDC, 90 * USDC)
            .expect("buy");
        let original_cost = l.layers[0].remaining_cost;
        let first = l
            .sell(1, buy.quantity / 2, 110 * USDC, 20, 1)
            .expect("sell");
        let rest = l.layers[0].quantity;
        let second = l.sell(1, rest, 120 * USDC, 20, 2).expect("sell");
        assert_eq!(l.layers[0].remaining_cost, 0);
        assert_eq!(
            first.pnl + second.pnl,
            first.cash_flow + second.cash_flow - original_cost as i64
        );
        assert_eq!(l.parked_profit, (first.pnl + second.pnl) as u64);
        assert_eq!(l.available(), l.cash - l.reserve - l.parked_profit);
    }
    #[test]
    fn open_losses_are_in_equity() {
        let mut l = Ledger::new(1000 * USDC, 400 * USDC, 4).expect("valid");
        l.buy(150 * USDC, 100 * USDC, 0, 0, "v1", 120 * USDC, 90 * USDC)
            .expect("buy");
        assert_eq!(l.equity(50 * USDC).expect("equity"), 925 * USDC);
    }
}
