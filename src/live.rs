//! Real-money accounting, kept separate from simulated balances.
pub use crypto_bros_core::ledger::reserved_part;
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
    #[serde(default)]
    pub reserved_quantity: u64,
    pub cost_usdc: u64,
    pub cost_satang: u64,
    pub entry_fee_usdc: u64,
    pub entry_fee_satang: u64,
    pub target_price: u64,
    pub review_price: u64,
    pub opened_at: i64,
    pub model: String,
}
impl RealLayer {
    pub fn tradable_quantity(&self) -> u64 {
        self.quantity.saturating_sub(self.reserved_quantity)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolFunding {
    pub received_sol: u64,
    pub source_usdc: u64,
    pub imported_sol: u64,
    pub imported_cost_usdc: u64,
    pub imported_cost_satang: u64,
    pub recorded_at: i64,
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
    #[serde(default)]
    pub entry_context: Option<crate::indicator_strategy::EntryContext>,
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
    #[serde(default)]
    pub entry_strategy: crate::indicator_strategy::EntryStrategy,
    #[serde(default)]
    pub reward_trades: Vec<crate::indicator_strategy::RewardTrade>,
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
    /// Immutable evidence for the selected model, independent of later research.
    /// Old installations have no evidence and must explicitly select a passing candidate.
    #[serde(default)]
    pub advice_research: Option<crypto_bros_core::research::Research>,
    pub last_advice_candle: i64,
    #[serde(default)]
    pub reserve_bps: u16,
    #[serde(default)]
    pub sol_funding: Option<SolFunding>,
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
            entry_strategy: Default::default(),
            reward_trades: Vec::new(),
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
            advice_research: None,
            last_advice_candle: 0,
            reserve_bps: 0,
            sol_funding: None,
        })
    }
    pub fn select_advice(
        &mut self,
        candidate: &crypto_bros_core::research::Research,
    ) -> Result<(), &'static str> {
        if self.pending.is_some() {
            return Err("Resolve the pending order before changing the advice model");
        }
        candidate.live_eligibility()?;
        self.advice_model = Some(candidate.model.clone());
        self.advice_policy = candidate.policy.clone();
        self.advice_research = Some(candidate.clone());
        self.entry_strategy = crate::indicator_strategy::EntryStrategy::ResearchModel;
        Ok(())
    }
    pub fn entry_policy(&self) -> crypto_bros_core::model::Policy {
        if self.entry_strategy == crate::indicator_strategy::EntryStrategy::RsiMacd {
            // Start small: 25% of the amount already bounded by cash, reserve,
            // exposure and remaining slots. Reward memory cannot enlarge it.
            crypto_bros_core::model::Policy {
                buy_size_bps: 2500,
                ..Default::default()
            }
        } else {
            self.advice_policy.clone()
        }
    }
    pub fn advice_eligibility(&self) -> Result<(), &'static str> {
        let model = self
            .advice_model
            .as_ref()
            .ok_or("ยังไม่ได้เลือกสูตรที่ผ่านการทดสอบสำหรับเสนอซื้อ")?;
        let research = self
            .advice_research
            .as_ref()
            .ok_or("สูตรเดิมไม่มีหลักฐานผลทดสอบที่ผูกไว้ กรุณาเลือกสูตรที่ผ่านการทดสอบใหม่ก่อนเสนอซื้อ")?;
        research.live_eligibility()?;
        if model != &research.model || self.advice_policy != research.policy {
            return Err("สูตรหรือพารามิเตอร์ไม่ตรงกับหลักฐานทดสอบ กรุณาเลือกสูตรที่ผ่านใหม่");
        }
        Ok(())
    }
    pub fn equity(&self, price: u64) -> Result<u64, String> {
        self.layers.iter().try_fold(self.cash, |sum, l| {
            sum.checked_add(ratio(l.quantity, price, SOL)?)
                .ok_or("Equity overflow".into())
        })
    }
    /// Explicitly reallocate recorded assets. This does not sign, swap, or
    /// manufacture a chain fill. Original funding is apportioned, not duplicated.
    pub fn allocate_80_20(
        &mut self,
        balances: &crate::chain::Balances,
        received_sol: u64,
        source_usdc: u64,
        price: u64,
        atr: u64,
        rate: &ThbRate,
        now: i64,
    ) -> Result<(), String> {
        if self.pending.is_some() || self.reconciliation_error.is_some() {
            return Err("จัดการรายการค้างและตรวจยอดก่อนเปลี่ยนการแบ่งทุน".into());
        }
        let importing = received_sol > 0 || source_usdc > 0;
        if self.sol_funding.is_some() || (self.reserve_bps != 0 && !importing) {
            return Err("บันทึกการแบ่ง 20/80 แล้ว ไม่สามารถนำ SOL ก้อนเดิมมาบวกทุนซ้ำ".into());
        }
        if (importing && (received_sol == 0 || source_usdc == 0))
            || price == 0
            || atr == 0
            || !rate.fresh(now)
            || !(0..120).contains(&(now - balances.checked_at))
            || balances.wrapped_sol != 0
            || balances.usdc < self.cash
            || source_usdc > self.source_total_usdc.saturating_sub(self.initial_usdc)
        {
            return Err("ตรวจยอด SOL ที่ได้รับและ USDC ต้นทุนจากก้อนเดิม แล้วรีเฟรชยอดกระเป๋า".into());
        }
        let tracked = self.layers.iter().try_fold(0_u64, |sum, l| {
            sum.checked_add(l.quantity).ok_or("SOL overflow")
        })?;
        let external = balances
            .sol
            .checked_sub(tracked)
            .ok_or("ยอด SOL ต่ำกว่าที่บันทึก")?;
        let quantity =
            (received_sol.min(external) - reserved_part(received_sol.min(external), 2000)).min(
                external.saturating_sub(
                    self.config
                        .sol_gas_reserve
                        .saturating_add(self.config.max_network_fee),
                ),
            );
        if importing
            && (quantity == 0
                || self
                    .layers
                    .iter()
                    .filter(|l| l.tradable_quantity() > 0)
                    .count()
                    >= self.config.max_layers)
        {
            return Err("SOL ไม่พอหลังกันค่าธรรมเนียม หรือจำนวนไม้เต็ม ยังแบ่งก้อนนี้เข้าเทรดไม่ได้".into());
        }
        let cost = if importing {
            ratio(source_usdc, quantity, received_sol)?
        } else {
            0
        };
        let cost_thb = ratio(self.source_total_cost_satang, cost, self.source_total_usdc)?;
        if importing && (cost == 0 || cost_thb == 0) {
            return Err("ทุนก้อนนี้เล็กเกินกว่าจะบันทึกต้นทุนได้".into());
        }
        let mark = ratio(quantity, price, SOL)?;
        let basis_price = if importing {
            ratio(cost, SOL, quantity)?
        } else {
            0
        };
        let mut next = self.clone();
        if next.reserve_bps == 0 {
            next.reserve_bps = 2000;
            next.config.reserve = reserved_part(next.cash, 2000);
            for layer in &mut next.layers {
                layer.reserved_quantity = reserved_part(layer.quantity, 2000);
            }
        }
        let id = next
            .layers
            .last()
            .map_or(Some(1), |l| l.id.checked_add(1))
            .ok_or("Layer id overflow")?;
        let policy = if next.advice_eligibility().is_ok() {
            next.advice_policy.clone()
        } else {
            Default::default()
        };
        let target_distance = (atr as f64 * policy.take_profit_atr) as u64;
        let review_distance = (atr as f64 * policy.review_atr) as u64;
        if importing {
            next.layers.push(RealLayer {
                id,
                quantity,
                reserved_quantity: 0,
                cost_usdc: cost,
                cost_satang: cost_thb,
                entry_fee_usdc: 0,
                entry_fee_satang: 0,
                target_price: basis_price
                    .checked_add(target_distance)
                    .ok_or("Target overflow")?,
                review_price: price.saturating_sub(review_distance).max(1),
                opened_at: now,
                model: "imported-sol-from-original-capital".into(),
            });
        }
        // Mark contributions at entry into bot tracking, so deposits are not bot profit.
        next.initial_usdc = next
            .initial_usdc
            .checked_add(mark)
            .ok_or("Capital overflow")?;
        next.initial_reference_satang = next
            .initial_reference_satang
            .checked_add(rate.satang(mark)?)
            .ok_or("Capital overflow")?;
        next.initial_cost_satang = next
            .initial_cost_satang
            .checked_add(cost_thb)
            .ok_or("Cost overflow")?;
        let exposure = next.layers.iter().try_fold(0_u64, |sum, l| {
            sum.checked_add(ratio(l.tradable_quantity(), price, SOL)?)
                .ok_or_else(|| "Exposure overflow".to_string())
        })?;
        next.config.trading_budget = next
            .cash
            .saturating_sub(next.config.reserve)
            .checked_add(exposure)
            .ok_or("Budget overflow")?;
        next.config.max_buy = next.config.trading_budget / next.config.max_layers as u64;
        if importing {
            next.sol_funding = Some(SolFunding {
                received_sol,
                source_usdc,
                imported_sol: quantity,
                imported_cost_usdc: cost,
                imported_cost_satang: cost_thb,
                recorded_at: now,
            });
        }
        *self = next;
        Ok(())
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
                .any(|l| l.tradable_quantity() > 0 && price <= l.review_price)
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
            || self
                .layers
                .iter()
                .filter(|l| l.tradable_quantity() > 0)
                .count()
                >= self.config.max_layers
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
                reserved_quantity: reserved_part(output, next.reserve_bps),
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
            if let Some(entry) = order.entry_context.as_ref().filter(|c| c.buy_ready()) {
                next.reward_trades
                    .push(crate::indicator_strategy::RewardTrade {
                        layer_id: id,
                        entry: entry.clone(),
                        sold_sol: 0,
                        disposed_cost_usdc: 0,
                        net_usdc: 0,
                        net_satang: 0,
                        exit_fills: 0,
                        complete: false,
                        failed_entry: false,
                    });
            }
            (id, 0, signed_difference(acquisition, disposed_basis)?)
        } else if order.side == "sell" {
            let layer = next
                .layers
                .iter_mut()
                .find(|l| l.id == order.layer_id)
                .ok_or("Tracked layer is missing")?;
            if input == 0 || input > layer.tradable_quantity() {
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
            next.config.reserve = next
                .config
                .reserve
                .checked_add(reserved_part(output, next.reserve_bps))
                .ok_or("Reserve overflow")?;
            let proceeds_thb = order.rate.satang(output)?;
            next.cash_basis_satang = next
                .cash_basis_satang
                .checked_add(proceeds_thb)
                .ok_or("Cost overflow")?;
            let gain = signed_difference(output, cost)?;
            next.parked_profit = next
                .parked_profit
                .checked_add({
                    let gain = output
                        .saturating_sub(cost)
                        .saturating_sub(fee_usdc)
                        .saturating_sub(entry_fee);
                    gain.saturating_sub(reserved_part(gain, next.reserve_bps))
                })
                .ok_or("Profit overflow")?;
            if let Some(reward) = next
                .reward_trades
                .iter_mut()
                .find(|r| r.layer_id == order.layer_id && !r.failed_entry)
            {
                let basis = cost.checked_add(entry_fee).ok_or("Reward basis overflow")?;
                let basis_thb = cost_thb
                    .checked_add(entry_fee_thb)
                    .ok_or("Reward basis overflow")?;
                let net = signed_difference(
                    output,
                    basis.checked_add(fee_usdc).ok_or("Reward fee overflow")?,
                )?;
                let net_thb = signed_difference(
                    proceeds_thb,
                    basis_thb
                        .checked_add(fee_satang)
                        .ok_or("Reward fee overflow")?,
                )?;
                reward.sold_sol = reward
                    .sold_sol
                    .checked_add(input)
                    .ok_or("Reward quantity overflow")?;
                reward.disposed_cost_usdc = reward
                    .disposed_cost_usdc
                    .checked_add(basis)
                    .ok_or("Reward basis overflow")?;
                reward.net_usdc = reward.net_usdc.checked_add(net).ok_or("Reward overflow")?;
                reward.net_satang = reward
                    .net_satang
                    .checked_add(net_thb)
                    .ok_or("Reward overflow")?;
                reward.exit_fills += 1;
                reward.complete = layer.tradable_quantity() == 0;
            }
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
    pub fn record_failed_reward(
        &mut self,
        order: &PendingOrder,
        fee: u64,
        fee_thb: u64,
    ) -> Result<(), String> {
        if order.side == "sell" {
            if let Some(reward) = self
                .reward_trades
                .iter_mut()
                .find(|r| r.layer_id == order.layer_id && !r.failed_entry)
            {
                let net = reward
                    .net_usdc
                    .checked_sub(i64::try_from(fee).map_err(|_| "Reward fee overflow")?)
                    .ok_or("Reward overflow")?;
                let thb = reward
                    .net_satang
                    .checked_sub(i64::try_from(fee_thb).map_err(|_| "Reward fee overflow")?)
                    .ok_or("Reward overflow")?;
                reward.net_usdc = net;
                reward.net_satang = thb;
            }
        } else if let Some(entry) = &order.entry_context {
            self.reward_trades
                .push(crate::indicator_strategy::RewardTrade {
                    layer_id: 0,
                    entry: entry.clone(),
                    sold_sol: 0,
                    disposed_cost_usdc: order.input,
                    net_usdc: signed_difference(0, fee)?,
                    net_satang: signed_difference(0, fee_thb)?,
                    exit_fills: 0,
                    complete: true,
                    failed_entry: true,
                });
        }
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
            entry_context: None,
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
