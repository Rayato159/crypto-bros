# Model, execution and risk boundaries

## RSI/MACD monitoring and realized rewards (0.6)

The primary watch button explicitly selects `rsi-macd-v1`, a transparent rule strategy, separately from the ridge research strategy. It requires a fresh, continuous series of closed 15-minute candles: RSI14 in [30,55], RSI rising versus the previous bar, MACD above its signal and histogram rising. These are provisional product rules, not statistically validated thresholds. No training wait, forecast or fictitious passed backtest is attached to this mode. Indicator definitions are described by [Fidelity RSI](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/RSI) and [Fidelity MACD](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/macd); our conjunction and 55 upper bound are application choices.

Suggested buys use 25% of the already bounded cash/exposure/per-slot budget and the existing one-hour cooldown. Reserve 20%, hard caps, stale-data checks and Phantom approval still apply. The target distance is 2.5 ATR and review distance 3 ATR. At preparation and approval, the exact entry context must still match, and minimum received SOL valued at the target must cover the input, estimated exit fee/slippage (30 bps), estimated round-trip network fees and 0.20% headroom. This checks hypothetical target economics, not the likelihood of reaching the target. Actual profit-exit quotes must still cover both currencies. No guaranteed stop or automatic execution is implied.

Only finalized fills update reward memory. Each rule buy stores the closed-bar context before signing. Sales accrue `received USDC - disposed entry basis - allocated entry network fee - exit network fee`, with an independent THB result. Failed swaps' network fees are recorded too. Recoverable rent is not a fee. One layer is one completed case once its tradable portion is sold; protected SOL remains open and is not a realized reward. Partial sales never inflate the number of samples. Duplicate settlement receipts cannot repeat rewards. Old/imported/manual trades without captured context are not reconstructed as indicator evidence.

Similar cases use fixed five-point RSI buckets, MACD's sign and histogram/ATR deciles. The displayed score is the mean realized USDC return on disposed cost among completed cases; positive USDC and THB results are counted as wins. Failed entries have a separate count and their fees affect the realized total, not a fabricated completed trade. With zero completed cases the score is absent, not a predicted win rate. Scores update online after confirmations but do not change entry rules or limits. This is outcome memory, not Q-learning, deep learning or continuous retraining of the ridge model. Human feedback remains separate from realized financial reward.

## Forecast and policy

For a completed candle t, x contains `(RSI14-50)/50`, MACD histogram / close, four-bar return, ATR14 / close and log1p volume relative to its trailing mean. RSI and ATR use Wilder smoothing; MACD uses EMA12, EMA26 and signal EMA9. Indicators warm up for 50 bars. Each feature is standardized using training-period mean and scale only.

The ridge model estimates `r_hat = beta_0 + sum(beta_i * standardized_x_i)`, where the target is `close[t+4] / close[t] - 1`. Six coefficients are solved with regularized Gaussian elimination. This is a conditional statistical estimate, not a guaranteed tradable price.

Entry requires `r_hat - uncertainty_weight * calibration_RMSE - 2*(fee_bps+slippage_bps)/10000 > min_edge`, available budget, cooldown, layer capacity and hard risk checks. Take-profit price is simulated entry price plus `take_profit_atr * ATR`; the human-review price is entry minus `3 * ATR`. There is no globally optimal buy/sell price claim.

Costs currently use 20 bps fee and 10 bps slippage per side. This is a configurable-in-code approximation, not a verified Jupiter fee or gas estimate. Live-mode work must use a timestamped executable route, min-received limits, actual chain fees and fill reconciliation. Binance reference candles are a different market from Jupiter's on-chain execution.

## Time splits and optimization

- 55% chronological training; future labels cannot cross its boundary.
- 10% subsequent error calibration; labels end before the next block.
- 17% validation, evaluated as two chronological subperiods with four-bar gaps.
- Final 18% holdout, separated by a four-bar gap. Used only for evaluation/status.

64 reproducible bounded random-search trials vary minimum edge, error penalty, take-profit ATR, entry cooldown and buy/sell size fractions. Score = average validation return minus twice the worst drawdown minus the absolute difference in subperiod returns. Fees, reserve, layer cap, review ATR and hard drawdown limits remain outside optimization. Research uses 1,000 USDC, 20% receipt reserves and two tradable slots, separately from the default paper account below. The default policy is evaluated as a benchmark using the same trained model, not an independent buy-and-hold benchmark.

Every backtest signal uses the previous completed candle and fills at the next candle's open with simulated costs. A later closing price cannot prevent an earlier planned fill. Once review is triggered the simulation holds positions; it neither assumes a stop-loss fill nor a human response. Equity includes all open positions marked at price, without an assumed exit fee until sold.

The first ~1,000 bars are around ten days, with a short holdout. Few fills receive `insufficient_evidence`. `shadow_candidate` means only that this limited simulation passed its basic checks. It does not imply deployment readiness. Subsequent research requires a holdout starting after the previous search's last data timestamp. Old results are not reused to select new parameters, although overlapping history can still induce dependence between experiments.

## Accounting and controls

USDC is recorded in 10^-6 units and SOL in 10^-9 units. Multiplication uses checked conversions and u128 intermediates. Buy cost includes fees, partial sells allocate integer cost basis, and the final sale consumes all residual basis. Positive realized profits remain parked until explicitly released. Reserve cannot be deployed by the model.

Default paper account: 1,000 USDC, 400 reserve, at most four open layers, 150 USDC per initial layer. Layer budgets remain fixed after profit release; releasing profit restores available cash rather than automatically increasing order sizes. Equity drawdown of 10%, total loss of 50%, or a layer review breach pauses the engine. This can occur on a sampled price rather than the exact crossing; it is not a guaranteed liquidation price. Selling remains available with fresh data. Acknowledgement is not permission to exceed hard risk limits.

## Feedback and future research

The journal captures decisions, model version, forecast, estimated net edge, simulated fills and user ratings. Ratings are persisted only; no Q-learning or neural model is active. A future learner must separate user preference from realized market reward, account for open losses and costs, use longer out-of-time evaluation, and stay behind the same hard controls. Paper exploration must never become random actions with real funds.

Live execution uses independent transaction inspection, per-order Phantom signatures, persisted submission tracking, expiry handling and finalized balance reconciliation. Building or simulating this path is not proof of funded execution or profitability. No unattended signer or wallet-key custody is implemented.
