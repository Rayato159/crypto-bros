# Development log

## 2026-10-30 — 0.1.0 research preview

Implemented a Rust workspace: independent worker and SQLite persistence; Dioxus Web dashboard in Tauri; NSIS packaging; tray lifecycle; CLI controls; protected localhost API; read-only Phantom public-address pairing; public SOLUSDC data and Jupiter quote adapters; optional Resend alerts. Wallet secrets are prohibited by AGENTS.md and are absent from the configuration schema.

Core: causal RSI/MACD/ATR, ridge regression, train-only scaling, error calibration, 64-trial bounded parameter optimization with chronological validation/holdout, candidate-only activation, integer layer accounting, parked realized profit, hard risk pause and feedback records. Q-learning, deep learning and live transaction execution are not implemented. The 5% weekly goal is aspirational.

Corrected a backtest lookahead defect: a bar's closing price must not veto a fill at its opening. Added a regression scenario with a sudden crash, confirming planned fills still execute and review does not fabricate sales. Prevented alert acknowledgement from bypassing risk limits; rejected stale/future data; required a new holdout; isolated synthetic models from real market source switches; prevented entry during update quiescence. Failed persistence pauses and restores the last committed state where possible.

Verification: 10 core tests, 1 persistence test and 8 workstation tests passed (19 total). Restricted execution initially denied localhost sockets; rerunning with network access passed. The separate read-only internet test also passed: 999 completed SOLUSDC candles and a keyless Jupiter quote parsed successfully. Dioxus WASM bundling and Tauri release/NSIS packaging passed. CI workflow was added but has not run remotely.

Native installed-app verification passed: real market chart and indicators rendered; 64-trial research returned `insufficient_evidence` with zero holdout fills (not evidence of profit); candidate selection did not auto-start; manual paper buy used 150 USDC; 50% sale left 75 USDC basis; feedback count increased; Jupiter quote returned without a wallet; update check correctly reported no published channel. CLI start/status/stop worked. Closing the window hid it while the paper worker continued; restarting preserved the open layer/history and started paused. Actual Phantom extension authorization and a user's on-chain balance have not been exercised. No email credentials were configured, so email delivery remains unverified.

Windows startup correction: Windows PowerShell `Set-Acl` attempted an unnecessary security privilege. The runtime now persists only the directory access rules using `Directory.SetAccessControl`; an added restart/owner-access regression test passed. Inspection confirmed only the user and SYSTEM have directory access. No administrative elevation is requested by the app. Final UI copy was adjusted and the sidebar brand was stacked after visual inspection.

Installed and tested locally; installer is not Authenticode-signed, no updater signing key/channel is configured, and nothing is published. Final artifact hashes and installed binary comparisons are recorded in `artifacts/VERIFICATION.txt` after packaging. The tests used a separate data directory; the user-facing account starts fresh and paused.

Installer comparison accounts for Tauri's documented-in-source `__TAURI_BUNDLE_TYPE_VAR_UNK` → `__TAURI_BUNDLE_TYPE_VAR_NSS` patch. Exactly those three bytes differ between the build GUI and the installed GUI; all remaining bytes match. The worker is byte-identical. `scripts/verify-installed.mjs` rejects any extra difference. A reinstall preserved the isolated paper database hash.

No user secrets were read, no real wallet was controlled, no funds moved, no email sent, and no release was published. Test data uses isolated temporary directories.

## 2026-10-30 — Public release preparation

Created the public `Rayato159/crypto-bros` repository at the user's request. Excluded all local PowerShell/batch launch and build helpers, the Windows installer-comparison helper, and the Windows-specific CI wrapper from Git. Published build commands use a portable Node preparation step; CI uses Ubuntu for Rust tests and the WASM check. Existing Windows runtime support and the Windows installer remain part of the product.

Rebuilt the preview through `npm run build`: all 19 local tests passed, the Dioxus frontend bundled, and Tauri generated the NSIS installer. Rust formatting passed. No trading behavior changed. The worker hash is unchanged. The packaged GUI was rebuilt to reflect the preparation-command configuration. Publication is tracked by the GitHub release and CI status; a public installer does not enable signed in-app updates.

The rebuilt installer exited successfully, and its installed worker matched the build byte-for-byte. The installed GUI matched after only Tauri's expected NSIS package marker. Windows Authenticode status remains unsigned.

## 2026-10-30 — v0.1.0 published and independently downloaded

Published [v0.1.0 Research Preview](https://github.com/Rayato159/crypto-bros/releases/tag/v0.1.0) from `c5ab3fb6aee6f097d2c6063051d488e8dd039179` after uploading and checking a draft. The [main check](https://github.com/Rayato159/crypto-bros/actions/runs/41090350766) and [tag check](https://github.com/Rayato159/crypto-bros/actions/runs/41090455503) both passed Rust formatting, worker/core tests and the WASM build check on Ubuntu.

All three public assets were downloaded without authentication and matched local sizes and SHA-256 values. The published checksum manifest also verified both named files. GitHub normalizes spaces in asset names, so the manifest uses `Crypto.Bros_0.1.0_x64-setup.exe` (5,990,381 bytes; SHA-256 `d598e5e8577441f9ffa6712b081e3fb22e90594e3d7b997115182632c0445ba7`). The other assets are `VERIFICATION.txt` and `SHA256SUMS.txt`.

An anonymous inspection of the 92-file release source tree confirmed the excluded Windows helpers were absent. The release remains paper-only and unsigned; there is no configured in-app update channel. This documentation entry does not change the tagged source or installer.

## 2026-10-31 — v0.2.0 manually approved trading and THB cost basis

Added a separate integer real-money ledger, proportional historical THB funding allocation, USDC/THB reference valuation, native-SOL fee accounting, partial layer sales, parked profits and independently selected live advice models. Historical THB performance is separate from bot performance since onboarding. Personal funding amounts and addresses are not included in source or installers. Paper funds cannot flow into the real ledger.

Implemented Jupiter Metis exact-input order preparation, bounded legacy/v0 transaction parsing, associated-account and instruction restrictions, simulation, separate gas-reserve checks, and an expiring localhost Phantom approval page with a locally bundled SDK. The worker verifies the exact signed message and wallet signature, commits the pending signature before submission, and books only finalized RPC balance changes. Unknown submissions remain pending; old missing history requires a history-capable RPC. Updates refuse unresolved signed orders. Restarts preserve pending work and pause new entries. No unattended signing or raw wallet-key custody is implemented.

Live advice includes existing RSI/MACD ridge-model features and executable cost checks. Manual test orders bypass the signal only. Hard budget, layer, reserve, fee, quote-deviation and loss limits remain enforced. Live risk reviews queue the optional email path but never simulate a guaranteed stop-loss sale. Feedback is retained; no Q-learning or automatic policy promotion is claimed.

Changed desktop packaging to MSI through Tauri/WiX. Added visible startup failures, graceful old-worker quiescence/backup during version replacement, and per-version database backups. The updater remains unconfigured and Windows artifacts remain unsigned. No real wallet was signed for, no funds moved, and no email was sent during development. Synthetic packet/account tests and mocked browser approval tests are distinct from actual Phantom/mainnet verification. Final build, artifact and publication results are recorded below when completed.

### v0.2.0 packaged verification

All 32 Rust tests and 3 bundled browser-wallet tests passed. The separate read-only public checks passed for Binance SOL/USDC candles, Jupiter quotes, Coinbase USDC/THB and Solana mainnet identity. The public RPC check caught an incorrect mainnet genesis constant; the final binary contains the verified value. Rust formatting, the WASM check and the MSI build passed; npm reported no known dependency vulnerabilities at verification time.

Administrative extraction of the final MSI succeeded. Its worker is byte-identical to the release build; its desktop differs only by Tauri's expected `UNK` to `MSI` bundle marker. The package contains the two intended executables, without personal data or configuration. The final unsigned Windows x64 MSI is 9,502,720 bytes, SHA-256 `2f5667f8fac3758b55428257080f08e97088d72d76f55588e9f5eed670a5e4a0`.

The packaged desktop started with an isolated data directory while an existing worker remained running. This check exposed a fixed-port collision, corrected by allowing the desktop worker to select an available port. The final app fetched real candles, rendered indicators and displayed proportional THB funding allocation using generic test amounts. Full MSI system installation, actual Phantom approval, funded transaction execution, email delivery and signed updater delivery are not established by these checks.

### v0.2.0 publication and cleanup

Published [v0.2.0 Manual Trading Preview](https://github.com/Rayato159/crypto-bros/releases/tag/v0.2.0) from `7ecffc64cc3182760a4ad492f8e97362df25ae3b` after checking the draft's three assets. Anonymous downloads of the MSI, `VERIFICATION.txt` and `SHA256SUMS.txt` matched local byte sizes and SHA-256 hashes. The 102-path tagged source tree excludes Windows helper scripts and private data filenames. The [main check](https://github.com/Rayato159/crypto-bros/actions/runs/41101470015) and [tag check](https://github.com/Rayato159/crypto-bros/actions/runs/41101469972) were still queued at the final publication check; local test success is not a claim that CI passed.

Restarted the isolated packaged instance, verified persisted market state with trading paused, and stopped that instance before cleanup. Removed 6,545,518,960 bytes of generated builds and this run's disposable test files after validating all cleanup paths. Retained the release artifacts and left the pre-existing user worker and application data untouched. This documentation-only follow-up does not change the tagged installer source.

## 2026-10-31 — v0.3.0 Trade Watch

Added an opt-in, read-only trade monitor with Thai buy/sell cards and native desktop notifications. Starting the monitor refreshes public market, FX and wallet balances, and selects an initial research candidate only when no live model exists. Existing models are retained; later research is not silently promoted. Buying is sized from the remaining budget, reserve, parked profit and remaining layer slots. Per-layer profit targets and loss-review prices produce sell cards; global risk reviews take priority. These are recommendations, never automatic signatures or guaranteed stop-loss fills.

Every card is checked again before refreshing data, after refresh and after quote preparation. Stale data, missing gas, withdrawn cash, full layers, reconciliation failures and pending submissions block incompatible actions. Profit-target exits additionally require minimum quoted proceeds to cover entry basis, entry fees and estimated exit network costs. Risk exits remain available for human review. Notification claims are serialized, remembered for 15 minutes and persisted; snoozing does not waive risk rules. Restarts and update quiescence disable monitoring while retaining the account and pending transaction state.

Added the official Tauri notification plugin. Clicking a notification focuses the live panel; only the explicit card button prepares a quote and opens Phantom. Notification delivery is subject to Windows settings. The API reports a request to Windows, not confirmed display. Windows only reports notification clicks while the toast is onscreen; older Action Center items may require opening the app from the tray.

Enlarged text and buttons, translated live account labels, explained bot versus historical THB P&L and untracked native SOL, and collapsed manual research/order tools into an advanced section. Buttons now show busy text and a fixed, visible result/error message. A single start button replaces the research/activation sequence for ordinary monitoring. Updated the README with the short workflow and metric meanings.

Verification so far: all 44 Rust tests and the 3 bundled browser-wallet mock tests passed. New cases cover concurrent notification claims, stale clicked cards, capital limits, source freshness, restart migration and fee-aware target exits. The first MSI build and Dioxus WASM build succeeded; the final package includes the later exit-cost guard. Packaged UI, artifact and publication results follow below. No funded transaction or Phantom signature was submitted during this work.

### v0.3.0 packaged verification

The final Dioxus/Tauri MSI build, formatting check, 44 Rust regressions, 3 browser-wallet mocks and 2 read-only public endpoint tests passed. Administrative extraction verified the worker byte-for-byte and the desktop after normalizing only Tauri's bundle marker. The MSI contains only the intended two executables. The unsigned MSI is 8,830,976 bytes, SHA-256 `f38e4b84b2c41f53d0285c5b51561812d2ab2d19bea4a36234de9c4af5553ed8`.

Opened the packaged desktop against a clearly labeled, isolated UI fixture without RPC, Phantom or financial submission access. Visually checked Thai typography, capital metrics, collapsed advanced tools, a buy card, start/stop, snooze, immediate busy state and a fixed visible rejected-command message. The automatic notification path and neutral test button returned successful requests to Windows; actual toast appearance/click delivery was not observed, so installed Windows notification delivery remains unverified. The existing funded app and its state were not modified. Full MSI system installation, actual Phantom approval, funded execution, email and signed updater delivery are not established by these checks.

### v0.3.0 publication and cleanup

Published [v0.3.0 Trade Watch Preview](https://github.com/Rayato159/crypto-bros/releases/tag/v0.3.0) from `cc6333dd3d53416e441fb91769a97a028dfcdd8a`. Anonymous downloads of the MSI, `VERIFICATION.txt` and `SHA256SUMS.txt` matched the local files exactly. GitHub reported success for both the [release tag](https://github.com/Rayato159/crypto-bros/actions/runs/41115320841) and [main](https://github.com/Rayato159/crypto-bros/actions/runs/41115321276) at that source commit. Checked all 105 tagged source paths for private data filenames and disallowed Windows helper scripts.

Stopped the isolated UI fixture and packaged test instance. Removed 5,921,767,699 bytes of generated builds and disposable test files after validating absolute paths, reparse points and preserved installer checksum. Retained release artifacts and visual proof outside `target`; user application data and existing releases remain intact. This documentation-only follow-up does not change the tagged installer source.


## 2026-10-31 — v0.4.0 wallet readout and signed updates

The user designated added SOL as network-fee funds and withdrew a daily-profit target because the app runs intermittently. Live trading now shows actual native SOL, the portion outside tracked layers, THB valuation, total wallet value, recorded network fees and recoverable token-account rent separately. External deposits never change the bot ledger or budget. The on-chain wallet mark is not charged recorded fees a second time. Retired the unused weekly-target display/state field; targets remain per layer with cumulative net performance, without forced daily or weekly returns. The live page warns that positions remain exposed while the computer is off.

Added readable RSI/MACD/Signal/Histogram cards, decision reasoning, forecast net edge and timestamps for closed 15-minute candles. The Overview candlestick chart now has mouse/click/keyboard tooltips with Bangkok opening time, OHLC, open-to-close change and SOL volume. Synthetic and stale data cannot be presented as a fresh live signal.

The desktop now pins the publisher's public key, enforces signed version metadata and uses a public HTTPS static update channel. Private signing material stays in an ignored, restricted publisher directory and is only passed to Tauri by path. The agent did not inspect it. Release signature verification accepts only public artifact inputs. The updater reports progress and errors, prevents concurrent operations, rechecks the reviewed version, verifies the download before quiescing, and refuses installation if backup or worker shutdown fails. Older 0.3 clients require one manual MSI upgrade to bootstrap this key and channel.

### Verification before publication

The optimized Dioxus/Tauri build and public artifact signature verification passed, including rejection of modified bytes and a different announced version. The MSI administratively extracts only the desktop and worker; worker bytes match and the desktop differs only by Tauri's expected MSI bundle marker. All 48 Rust tests passed, including external gas-deposit accounting, no repeated fee deduction, updater stage ordering and a backup failure that leaves trading paused. The first restricted test run failed two loopback tests due to Windows permission errors; rerunning with authorized local networking passed. No production behavior was changed to accommodate the restriction. Formatting passed. Visual inspection and publication results follow after completion. No funded transaction has been submitted.


### v0.4.0 packaged verification

Also passed 3 browser-wallet mock tests and 2 read-only public-provider tests (999 closed candles, quote-only Jupiter, USDC/THB and Solana mainnet identity). Inspected the packaged desktop with isolated synthetic balances and no financial routes: wallet/gas values, readable Thai indicator cards, explicit stale-data warning, candle mouse selection and Tab-to-next-candle tooltip. An unavailable update endpoint produced a visible error. Visual inspection caught small inherited paragraph/error text, which was increased before the final build. Packaging now removes only validated generated frontend outputs before rebuilding, preventing obsolete hashed JS/WASM assets from accumulating. The final package has exactly one frontend JS/WASM pair.

The final MSI is 8,867,840 bytes, SHA-256 `578b112699fc5d3c69729f728b2da7301e6cf0d14d34e3fd771caba6326341a9`. Its public signature, version binding, tamper rejection and extracted payload were verified again after the final packaging correction. The Tauri updater signature does not supply Windows Authenticode. Full MSI system installation, actual in-app replacement and funded Phantom execution remain unverified; the user's existing app and wallet were not used for these checks.


### v0.4.0 publication and channel activation

Published [v0.4.0](https://github.com/Rayato159/crypto-bros/releases/tag/v0.4.0) from `a715457348918b57728f184087d011d9ae16f574`. Anonymous downloads of all five assets (MSI, public signature, static manifest, verification record and checksums) matched local bytes exactly; the downloaded MSI passed public-key signature/version verification and tamper rejection. Only after that verification was the identical manifest added to `updates/windows.json` for channel activation. All 114 tagged source paths were checked for private-data filenames and prohibited Windows helper scripts. Post-activation desktop check and CI results follow below when verified.


The production raw-GitHub channel was fetched anonymously and its JSON, version and signature matched the released manifest; Git normalized only CRLF to LF. The final packaged desktop's actual **Check updates** command returned the latest-version result from that channel. This validates channel access/parsing, separately from the already verified download signature; a complete in-app MSI replacement was not attempted.

Stopped the isolated fixture server and packaged test instance, then removed 5,082,248,169 bytes of generated builds and disposable test files after validating absolute workspace paths and rejecting reparse points. Rechecked the preserved MSI hash and publisher-key existence without reading key contents. Release artifacts and screenshots remain under `artifacts/v0.4.0`; source, private publisher material and installed user data were preserved.

Both source CI runs completed successfully: [v0.4.0 tag](https://github.com/Rayato159/crypto-bros/actions/runs/41120967751) and [main](https://github.com/Rayato159/crypto-bros/actions/runs/41120966804), at the exact released commit. This documentation follow-up does not change the tagged installer.

## 2026-10-31 — v0.5.0 evidence gates, reserves and position sizing

Closed live candidate promotion gaps. Explicit selection, first monitoring start, advice, quote preparation and outstanding model-order approval all require recorded passing research tied to the exact selected model and policy. Unknown, rejected, insufficient, malformed and legacy evidence fails closed. Later research cannot replace or certify an existing model silently. Existing positions retain sell/risk monitoring without an eligible buy model. Legacy accounts deserialize without manufacturing approval; already submitted signatures still reconcile normally.

Added an explicit 20/80 allocation action, with persistent protected USDC and per-layer SOL quantities. Each subsequent receipt is split; repeated partial sales cannot consume the protected SOL remainder. Reserves can grow beyond 20%, are never recursively treated as trade funds, and the existing SOL network-fee floor still applies. Limits change only in the user-invoked allocation action, not optimization. External SOL can be recorded once from its original received quantity and USDC acquisition cost within the already recorded source capital; only the available tradable portion is apportioned into a layer. Original THB cost is proportionally allocated, cash is not duplicated, imported positions are not chain fills, and contribution marks are not bot profit. Unknown-basis SOL remains outside the trading ledger for fees. No installed account was changed during development.

Expanded bounded validation search to buy and profit-exit size fractions. Research execution version 2 simulates the same 20% receipt reserves and two tradable slots; old evidence cannot approve the changed execution rules. Eight distinct buy entries are required in holdout, so slicing one position into many exits cannot manufacture sample count. Buy sizing is applied only after cash, reserve, layer and exposure caps; risk reviews offer the whole tradable portion, never reserved SOL. Partial profit exits must cover proportional entry basis/fees and quoted exit cost in both USDC and reference THB. Backtests still use a standardized 1,000-USDC account and swap/slippage estimates; actual network costs are checked at quote time and profitability is not established by these tests.

The first restricted test run hit two Windows loopback permission errors. The authorized local-network rerun passed those tests and exposed an old approval fixture without model evidence; that fixture now explicitly represents a manual order while new tests separately cover model approval rejection. An intermediate full run passed 62 Rust tests and the Dioxus WASM check; final expanded checks and artifact results will be recorded below. No funded order, Phantom signature or email was sent.

### v0.5.0 verification

The final expanded checks passed 63 Rust tests, the Dioxus WASM check, 3 wallet-page mock tests and 2 read-only public-provider tests (999 closed candles, Jupiter quote without a taker/signature, USDC/THB and Solana mainnet identity). Packaged-desktop UI inspection used an isolated synthetic HTTP fixture with no financial routes. It caught a missing desktop command allowlist entry for the new allocation button; the entry was added and the MSI rebuilt before publication. The final extracted app successfully submitted the allocation form through its native bridge, displayed protected balances and imported basis, kept failed-model monitoring visible and displayed an actionable failed-selection error. UI fixture responses are not funded chain execution or proof that a real model passes.

The final MSI is 8,880,128 bytes, SHA-256 `41d7776c76cf34459a2dfcd2c30b239c625ee7ae15a732a29d8c73f3e873f613`. Its public updater signature and version binding passed; modified bytes and another announced version were rejected. Administrative extraction contains only desktop and worker binaries. Worker bytes match the build; desktop bytes match after Tauri's expected MSI bundle marker. Full MSI system installation, actual in-app replacement and funded Phantom execution remain unverified. The installed user app and its data were untouched; no Windows helper scripts, personal state or signing keys are included in the release.

### v0.5.0 publication

Published [v0.5.0](https://github.com/Rayato159/crypto-bros/releases/tag/v0.5.0) as a preview at source/tag commit `97b0841163cb2ec043c41e10c5b2f0e5ce153016`. Both push CI runs passed at that exact commit: [41129162252](https://github.com/Rayato159/crypto-bros/actions/runs/41129162252) and [41129162169](https://github.com/Rayato159/crypto-bros/actions/runs/41129162169). All five public assets were downloaded anonymously and compared byte-for-byte with the local release; the downloaded MSI also passed public signature, tamper and version checks. Channel activation commit `9eedadc` followed those public checks. The public static channel matches the released `latest.json` byte-for-byte, and the final packaged desktop's real channel check reported that 0.5.0 is current. This checks discovery, not installer replacement.

## 2026-10-31 — v0.5.1 closed wrapped-SOL inspection and sale explanations

Reproduced the user's `Unexpected token account program` in a deterministic account fixture before changing production code. The SPL Token processor drains lamports, assigns System Program and resizes data to zero during CloseAccount; simulation can expose this emptied WSOL account rather than null. The parser now accepts only that exact zero-lamport, non-executable, empty-base64 System representation for the wallet's canonical WSOL account. Nonempty or funded accounts, foreign programs, missing required fields and USDC accounts still fail closed. This also correctly represents an already-closed canonical WSOL account before a swap. See the [official Token processor](https://github.com/solana-program/token/blob/main/program/src/processor.rs), `process_close_account` and `delete_account`.

Regression checks cover buy/sell cash and SOL deltas, network fees and rent, null/empty-account handling and invalid-account rejection. Error context now distinguishes pre-swap balances from simulated post-swap balances. No user session, private RPC response, wallet secret or funded transaction was read or used to reproduce the issue; the exact user's RPC response remains unobserved.

Target cards now distinguish reaching a price from recovering both USDC and historical baht basis. The actual profit-sale quote gate remains unchanged, with a clearer rejection explaining both currencies. The holdings table uses Thai labels, marks imported SOL, explains gas SOL outside a layer and explains why an imported layer's historical-cost target can fall below its import-price review threshold. Existing stored price thresholds and reserves are not rewritten. Manual-sale buttons are larger and explicitly say they prepare an order; research retry copy explains the fresh-data wait without promising an ineligible old model can continue buying.

Release verification and publication are recorded after the checks below; no funded sale or profitability claim is implied by fixtures or builds.

Verification so far: 67 workspace Rust tests passed, including the desktop update failure-path test; WASM check and 3 wallet-page mock tests passed. Two loopback tests needed the authorized local-network test run after Windows denied the restricted run. The compiled release frontend was inspected in a hidden browser with an isolated synthetic bridge and no financial routes: target/cost warning, imported-layer explanation, 16px/48px sale buttons and the fresh-candle error were visible. Native-app testing is deliberately deferred while the user plays a game; no native app was opened, focused or installed. Build jobs are limited to two at below-normal process priority.

### v0.5.1 artifact and publication verification

Published [v0.5.1](https://github.com/Rayato159/crypto-bros/releases/tag/v0.5.1) at source/tag commit `5dbe5662447e05a844d7e6eec6ef94c51474963d`. MSI: 8,888,320 bytes, SHA-256 `db1a8414d55284d140f511969e9102545eb3c280ed5334b23f5aca5c444e95f5`. Public signature/version validation passed and rejected changed bytes and a different announced version. Silent administrative extraction contained only worker and desktop; their bytes match the build with the expected Tauri MSI marker adjustment. Restricted extraction initially could not access Windows Installer; the authorized silent extraction succeeded without opening or installing the app.

Both exact-source CI runs passed: [41133780274](https://github.com/Rayato159/crypto-bros/actions/runs/41133780274) and [41133995491](https://github.com/Rayato159/crypto-bros/actions/runs/41133995491). All five public release assets downloaded anonymously and matched local hashes. The downloaded MSI passed signature, tamper and wrong-version checks again. Only then was the identical release manifest prepared for the static update channel. No Windows scripts, credentials or user state were published. Full installation, actual updater replacement, native UI launch for this patch and funded swaps remain unverified; these were not performed on the user's active machine.
