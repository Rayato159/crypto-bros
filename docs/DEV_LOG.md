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
