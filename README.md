# Crypto Bros

Rust desktop workstation for SOL/USDC research and **paper trading**. Dioxus Web runs inside Tauri; the Rust worker keeps running independently of the window. SQLite stores layers, simulated fills, models and feedback locally.

**0.1.0 is a research preview. It cannot sign or submit real transactions.** A +5% weekly return is a user goal, not a guaranteed return or a reason to relax risk limits. Nothing in this build establishes profitability.

## Run the Windows app

Download the installer and `SHA256SUMS.txt` from [GitHub Releases](https://github.com/Rayato159/crypto-bros/releases). Install for the current Windows user, then open **Crypto Bros** from Start. This preview is not Windows code-signed. Updates are manual; the in-app update channel is not configured.

1. **Refresh market** loads public SOL/USDC 15-minute candles. Alternatively choose **Load synthetic demo** to explore without a network connection. The app labels synthetic data explicitly.
2. **Run model research** trains the initial model and searches 64 policy candidates. Inspect the holdout results under Research.
3. **Select for paper trading**, then **Start paper engine**. Selecting a model alone does not start trading. Manual paper layers are also available.
4. Inspect each layer's cost, target, review threshold and realized fills. Close part or all of a layer. Profits remain parked until **Release parked profit**.
5. Close the window to hide it in the tray. Use **Pause paper trading** or **Stop bot and quit** from the tray. Restart always starts paused. Sleep, shutdown or lost connectivity prevents monitoring.

Paper capital starts at 1,000 USDC: 400 protected reserve, four layers with budgets up to 150 USDC including fees. Source changes are blocked after any fill to prevent mixing synthetic and market results. Use a separate `--data-dir` for a separate paper account; preserve your existing data.

## Wallet: use Phantom together with Jupiter

Phantom holds the wallet and authorizes signatures. Jupiter finds swap routes and provides quotes. They serve different roles; there is no need to choose one to replace the other.

1. Download the browser extension from [phantom.com/download](https://phantom.com/download). For stronger separation, use a separate browser profile and create a **new trading wallet with its own recovery phrase**, leaving your personal wallet intact.
2. Follow [Phantom's official new-wallet guide](https://help.phantom.com/articles/45135465489555): Create a New Wallet → Create a Recovery Phrase Wallet. Set the password and privately back up the phrase offline. Do these steps yourself. Never send the phrase to the app, chat, logs, support, or `.env`.
3. In Crypto Bros → Wallet, choose **Open Phantom connection**. The short-lived page opens in your default browser; that browser/profile must have Phantom. It requests only a public address. If your trading profile differs from the default browser, enter its public address in **Watch a public Solana address** instead.
4. **Refresh on-chain balance** reads SOL and Solana USDC balances. These balances are separate from paper capital. Connecting an address is not a proof of ownership and does not grant spending rights.
5. No deposit is needed for paper trading, model research or quote previews. Funding and wallet signing are future live-mode work. An additional account under the same recovery phrase separates balances, not recovery secrets.

**Do not put wallet seeds or private keys in `.env`.** The application has no raw-wallet-key configuration or export feature. Agent rules are in [AGENTS.md](AGENTS.md).

## APIs and optional email

| Service | Current use | Cost / setup |
|---|---|---|
| Binance public market data | Completed SOLUSDC candles and ticker, once per minute | Public endpoint; no API key. Regional/network availability may vary. |
| Jupiter Swap V2 | Quote-only USDC → SOL, no `taker`, signing or submission | Keyless requests supported; optional free API key. Free keyed plan lists 1 request/second. |
| Solana RPC | Public address balances | Default public RPC; rate limits apply. A private provider is optional. |
| Resend | Optional risk review emails | Requires your API key, verified sender and chosen recipient. Not configured by default. |

Official references: [Binance market-data-only](https://developers.binance.com/en/docs/binance-spot-api-docs/faqs/market_data_only), [Jupiter pricing](https://developers.jup.ag/pricing), [Jupiter quote/order API](https://developers.jup.ag/docs/swap/order-and-execute), [Resend send-email API](https://resend.com/docs/api-reference/emails/send-email). Provider pricing may change. Free data access does not mean free future trading: spread, slippage, network/priority fees and route fees still matter. Charts are rendered locally; no paid chart subscription is required.

The worker privately loads supported service settings from `.env` **in its data directory**, not from the repository or executable directory. The default Windows location is `%LOCALAPPDATA%\CryptoBros\data`. Copy `.env.example` there yourself and fill optional fields privately. Restart the worker afterward. Never give credentials to a coding agent. The application restricts this directory to the current OS user and SYSTEM on Windows; `.env` remains plaintext. OS credential-store support is not implemented yet.

Risk review starts at any open layer's 3× ATR review price or 10% equity drawdown; a 50% total-capital loss is also detected. The engine pauses and retains open positions. You decide whether to sell. Acknowledging an alert does not bypass a hard limit. When configured, email retries at most three times; provider acceptance is not proof of inbox delivery. No email can be delivered while the computer is asleep/offline. The protected USDC allocation is a budgeting rule, not protection against USDC depeg or chain/custody risk.

## Model and evidence

The initial model is ridge regression, not deep learning or Q-learning. Inputs: RSI14, MACD histogram, four-bar momentum, ATR/price, volume ratio. It predicts four 15-minute bars ahead. Training-only scaling, later error calibration, chronological validation and an untouched holdout reduce leakage. The optimizer maximizes a validation score penalizing drawdown and unstable results across two periods; it cannot optimize reserve, maximum layers or hard drawdown limits.

Candidates are never promoted automatically. Initial public history is only about ten days, which is insufficient to establish robustness. Scheduled research attempts every 72 hours after the first run and requires an entirely new holdout. Feedback is stored for later supervised/reward experiments; **it does not currently retrain the policy**. See [model design](docs/MODEL.md).

## Build and development

Requires Windows x64, Rust with the `wasm32-unknown-unknown` target, MSVC build tools, Node/npm, Dioxus CLI 0.7.2 and WebView2. The installer can bootstrap WebView2. Commit `Cargo.lock` and `package-lock.json` for reproducibility.

```sh
npm ci
cargo test --locked
cargo check -p crypto-bros-ui --target wasm32-unknown-unknown --locked
npm run build
# Optional read-only internet smoke check, never run by the default test suite:
cargo test --test public_data --locked -- --ignored --nocapture
```

CLI worker: `crypto-bros.exe serve`, `status`, `start`, `pause`, `stop`. Optional `--data-dir <folder>` separates accounts; worker `--port 0` chooses a free localhost port. The desktop also accepts `--data-dir <folder>` for isolated checks. The desktop starts its worker on port 48731. A single worker may own a data directory at a time. Do not inspect the private session file; use the CLI or app.

The portable Node preparation command builds the worker and web frontend; Tauri produces the current Windows installer in `target/release/bundle/nsis/`. Windows-specific local scripts are excluded from this repository. Desktop packaging on other platforms has not been verified.

Signed in-app update infrastructure is implemented but unconfigured in this preview. It still needs an HTTPS update manifest and publisher-controlled signing key. See [release instructions](docs/RELEASING.md) and [verification log](docs/DEV_LOG.md).
