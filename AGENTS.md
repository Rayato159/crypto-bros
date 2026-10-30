# Crypto Bros agent rules

## Wallet secrets: mandatory

- Never open, read, search, print, copy, decode, export, request, or transmit a user's wallet seed phrase, recovery phrase, private key, keypair, or keystore contents. This includes tool calls, screenshots, browser storage, clipboard, terminal output, logs, crash dumps, and tests.
- Never read `.env`, `.env.*` (except `.env.example`), credentials files, wallet/session stores, or operating-system credential stores. Do not enumerate environment variable values. Agents may check file existence and permissions without reading contents.
- Never ask the user to paste wallet secrets into chat. Never automate revealing or exporting secrets in Phantom.
- Prefer Phantom-managed signing: the application stores public addresses only and never requires a recovery phrase or private key. An agent wallet is a separate funded wallet, not access to the user's personal wallet.
- If a future implementation explicitly needs a secret, the user configures it privately. Application code may load a specifically named secret at runtime from an OS credential store (preferred) or a user-managed, ignored `.env` file. That does not authorize the coding agent to read the value. Do not implement raw-key custody without an explicit user request.
- `.env` is plaintext, not a vault. Keep it outside installer resources; ignore it in Git; restrict filesystem permissions; never send its contents to the frontend. Distribute only `.env.example` with empty values.
- Never include credentials, personal databases, trained user models, wallet files, or user logs in source commits, installers, release assets, or diagnostic bundles. Never log authorization headers or complete authenticated URLs.
- Public wallet addresses, transaction signatures, token mint addresses, and public API documentation are not signing secrets, but avoid publishing user-specific identifiers unnecessarily.

## Trading and release integrity

- Paper trading is the default. Never describe simulated fills, forecasts, backtests, or a successful build as verified live trading or profitability.
- Do not submit real transactions, move funds, or enable unattended signing without the user's explicit authorization for that scope.
- Reserve funds and hard risk limits cannot be changed by model optimization. Human-review loss triggers must not be simulated as guaranteed stop-loss fills.
- Use integer atomic units for the cash/position ledger. Record execution costs and mark open positions when reporting performance.
- Updates must quiesce trading, resolve pending submissions, back up persistent state, and reconcile before resuming. Preserve installed user data across releases.
- Keep updater signing keys private. Verify release artifacts before publication; distinguish built, tested, signed, published, and installed states.

## Implementation

- Primary implementation language: Rust. UI: Dioxus Web packaged in Tauri. Keep the trading engine independent of the UI.
- Avoid panicking operations in production paths; propagate actionable errors without leaking secrets.
- Keep meaningful implementation and verification notes in `docs/DEV_LOG.md`.
- Do not publish Windows-specific launch/build scripts or their CI wrappers. Keep local PowerShell, batch and installer-verification helpers ignored; use portable tooling for published build commands.
