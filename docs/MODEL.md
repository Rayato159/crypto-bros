# Model, execution and risk boundaries

## Forecast and policy

For a completed candle t, x contains `(50-RSI14)/50`, MACD histogram / close, four-bar return, ATR14 / close and log volume relative to its trailing mean. RSI and ATR use Wilder smoothing; MACD uses EMA12, EMA26 and signal EMA9. Indicators warm up for 50 bars. Each feature is standardized using training-period mean and scale only.

The ridge model estimates `r_hat = beta_0 + sum(beta_i * standardized_x_i)`, where the target is `close[t+4] / close[t] - 1`. Six coefficients are solved with regularized Gaussian elimination. This is a conditional statistical estimate, not a guaranteed tradable price.

Entry requires `r_hat - uncertainty_weight * calibration_RMSE - 2*(fee_bps+slippage_bps)/10000 > min_edge`, available budget, cooldown, layer capacity and hard risk checks. Take-profit price is simulated entry price plus `take_profit_atr * ATR`; the human-review price is entry minus `3 * ATR`. There is no globally optimal buy/sell price claim.

Costs currently use 20 bps fee and 10 bps slippage per side. This is a configurable-in-code approximation, not a verified Jupiter fee or gas estimate. Live-mode work must use a timestamped executable route, min-received limits, actual chain fees and fill reconciliation. Binance reference candles are a different market from Jupiter's on-chain execution.

## Time splits and optimization

- 55% chronological training; future labels cannot cross its boundary.
- 10% subsequent error calibration; labels end before the next block.
- 17% validation, evaluated as two chronological subperiods with four-bar gaps.
- Final 18% holdout, separated by a four-bar gap. Used only for evaluation/status.

64 reproducible bounded random-search trials vary minimum edge, error penalty, take-profit ATR and entry cooldown. Score = average validation return minus twice the worst drawdown minus the absolute difference in subperiod returns. Fees, reserve, layer cap, review ATR and hard drawdown limits remain outside optimization. The default policy is evaluated as a benchmark using the same trained model, not an independent buy-and-hold benchmark.

Every backtest signal uses the previous completed candle and fills at the next candle's open with simulated costs. A later closing price cannot prevent an earlier planned fill. Once review is triggered the simulation holds positions; it neither assumes a stop-loss fill nor a human response. Equity includes all open positions marked at price, without an assumed exit fee until sold.

The first ~1,000 bars are around ten days, with a short holdout. Few fills receive `insufficient_evidence`. `shadow_candidate` means only that this limited simulation passed its basic checks. It does not imply deployment readiness. Subsequent research requires a holdout starting after the previous search's last data timestamp. Old results are not reused to select new parameters, although overlapping history can still induce dependence between experiments.

## Accounting and controls

USDC is recorded in 10^-6 units and SOL in 10^-9 units. Multiplication uses checked conversions and u128 intermediates. Buy cost includes fees, partial sells allocate integer cost basis, and the final sale consumes all residual basis. Positive realized profits remain parked until explicitly released. Reserve cannot be deployed by the model.

Default paper account: 1,000 USDC, 400 reserve, at most four open layers, 150 USDC per initial layer. Layer budgets remain fixed after profit release; releasing profit restores available cash rather than automatically increasing order sizes. Equity drawdown of 10%, total loss of 50%, or a layer review breach pauses the engine. This can occur on a sampled price rather than the exact crossing; it is not a guaranteed liquidation price. Selling remains available with fresh data. Acknowledgement is not permission to exceed hard risk limits.

## Feedback and future research

The journal captures decisions, model version, forecast, estimated net edge, simulated fills and user ratings. Ratings are persisted only; no Q-learning or neural model is active. A future learner must separate user preference from realized market reward, account for open losses and costs, use longer out-of-time evaluation, and stay behind the same hard controls. Paper exploration must never become random actions with real funds.

Before live mode: independent transaction decoding/allowlisting, per-order wallet confirmation or explicitly authorized limited signer, idempotent submission tracking, blockhash expiry handling, confirmations and balance reconciliation, actual-cost modeling, stale-route rejection, daily/portfolio budgets and operational failure drills. No code in this preview calls a swap submit endpoint.
