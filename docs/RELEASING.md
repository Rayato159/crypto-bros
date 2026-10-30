# Release and update procedure

Version 0.2 builds a Tauri/WiX MSI installer. Application binaries and frontend assets are the only installer resources. User data belongs under LocalAppData and must survive reinstall/update. Never include `.env`, user databases, models, logs, connection/session files, wallet files or credentials in an artifact or source archive. Version 0.1's NSIS registration is a separate installer product; do not assume MSI removes that entry.

## Build a reviewable local artifact

1. Update package versions consistently in root/core/UI/desktop Cargo manifests, package.json and tauri.conf.json. Update user-visible version text.
2. Run `npm ci`, the WASM check, and `npm run build` (which includes worker/core tests). The installer is written under `target/release/bundle/`; copy only the intended installer and verification files into a separate release directory and generate their SHA-256 manifest.
3. Install into an isolated test directory/account and verify startup, market data, paper trades, restart persistence and tray stop. Confirm that an update starts paused and preserves existing data.
4. Inspect the exact artifact list and compare SHA256SUMS.txt against each file. Test and label architecture, signing, publishing and installation states separately.

`prepare_update` refuses unresolved signed transactions, cancels unsigned proposals, pauses entries, saves state and produces a SQLite `VACUUM INTO` backup including committed WAL data. Further commands that could resume trading are rejected. The desktop verifies a configured updater's download signature before quiescing, then requests shutdown, waits for the worker session to disappear, and launches installation. Restarts always pause new entries and preserve pending signatures for reconciliation. For a manual MSI update, resolve pending orders and stop the app before starting the installer; a manual installer is not the signed in-app update flow.

## Configure signed updates privately

The publisher supplies a Tauri updater keypair. **Agents must never open or print the signing private key.** Keep it in publisher/CI secret storage; do not commit it. Updater signatures are separate from Windows Authenticode code signing.

Supply a Tauri configuration override with the public verification key, HTTPS update manifest URL, `bundle.createUpdaterArtifacts` enabled, and signed-version verification retained. Keep generated configuration under ignored `target/` and pass it using `npm run build -- --config <path>`. Configure Tauri's signing-secret inputs privately in the build environment. Verify current [Tauri updater requirements](https://v2.tauri.app/plugin/updater/) including signed version metadata. Never weaken verification to make a release install.

Publish a draft release with the installer, its updater signature and SHA-256 manifest. Upload and verify all assets before making the release public; verify anonymous downloads and hashes afterward. The update manifest must point to the exact signed installer for `windows-x86_64`, include the version and signature, and be served over HTTPS. Do not publish a manifest pointing to absent assets. Test failed download/signature, existing pending work, failed backup and failed install paths before advertising automatic updates as production-ready.

The public distribution location is [GitHub Releases](https://github.com/Rayato159/crypto-bros/releases). The initial preview uses a manually installed unsigned installer, without an updater signature or manifest. Publishing a GitHub release does not configure in-app updates. No publisher signing key is configured; Settings reports that the updater is unavailable. Windows-specific helper scripts and their CI wrapper stay local and are excluded from Git.
