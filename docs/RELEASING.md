# Release and update procedure

Version 0.4 builds a Tauri/WiX MSI installer. Application binaries and frontend assets are the only installer resources. User data belongs under LocalAppData and must survive reinstall/update. Never include `.env`, user databases, models, logs, connection/session files, wallet files or credentials in an artifact or source archive. Version 0.1's NSIS registration is a separate installer product; do not assume MSI removes that entry.

## Build a reviewable local artifact

1. Update package versions consistently in root/core/UI/desktop Cargo manifests, package.json and tauri.conf.json. Update user-visible version text.
2. Run `npm ci`, the WASM check, and `npm run build` (which includes worker/core tests). The installer is written under `target/release/bundle/`; copy only the intended installer and verification files into a separate release directory and generate their SHA-256 manifest.
3. Install into an isolated test directory/account and verify startup, market data, paper trades, restart persistence and tray stop. Confirm that an update starts paused and preserves existing data.
4. Inspect the exact artifact list and compare SHA256SUMS.txt against each file. Test and label architecture, signing, publishing and installation states separately.

`prepare_update` refuses unresolved signed transactions, cancels unsigned proposals, pauses entries, saves state and produces a SQLite `VACUUM INTO` backup including committed WAL data. Further commands that could resume trading are rejected. The desktop verifies a configured updater's download signature before quiescing, then requests shutdown, waits for the worker session to disappear, and launches installation. Restarts always pause new entries and preserve pending signatures for reconciliation. For a manual MSI update, resolve pending orders and stop the app before starting the installer; a manual installer is not the signed in-app update flow.

## Signed update channel

The public verification key is pinned in `src-tauri/tauri.conf.json`; `requireSignedVersion` stays true. The HTTPS channel is `https://raw.githubusercontent.com/Rayato159/crypto-bros/main/updates/windows.json`. Do not substitute the `releases/latest` endpoint while releases are previews: GitHub excludes pre-releases from that alias.

The existing publisher key is under ignored `.publisher/updater.key`, protected to the publisher OS account and SYSTEM. **Agents must never open or print the private key. Never regenerate, replace or delete an existing publisher key as routine cleanup.** Its `.pub` companion is public. Keep a private backup: losing the key prevents existing clients from trusting future updates. No wallet keys are involved.

Use `npm run build:signed`. This portable script only checks existence and passes the key PATH to Tauri. It never opens signing material or outputs its contents. Tauri generates the MSI and `.msi.sig`, including signed version metadata. Do not put the key under `target`, `artifacts`, installer resources or source control. Keep `.publisher` untouched during cleanup. A source-only local build without a publisher key can disable updater artifact generation with an ignored configuration override; do not publish that unsigned build to the signed channel.

Before publishing:

1. Preserve the exact MSI and `.sig` outside `target`. Run `cargo run --example verify_update -- <MSI> <SIG> <VERSION>` to verify the public artifact signature, reject changed bytes and reject a different announced version.
2. Make a static manifest with the exact version, notes, RFC3339 publication date and `platforms.windows-x86_64.{url,signature}`. The signature is the CONTENTS of the public `.sig` file, never a path. Include the same manifest as `latest.json` with the release assets.
3. Upload the MSI, signature, `latest.json`, verification record and SHA-256 manifest to a draft release. Inspect all assets, publish, then anonymously download and verify every exact byte/hash.
4. Only AFTER assets are publicly verified, commit the identical manifest to `updates/windows.json` on main. The channel must never point at missing assets. Fetch the public channel and verify its version, URL and signature against the released files.
5. Test the installed desktop's channel check. Label signature/download validation, failure-path tests, full MSI system installation and actual in-app replacement separately; do not claim the last two from administrative extraction or mocked installer calls.

Old 0.3.0 clients lack the pinned key and endpoint; they need one manual MSI upgrade. User data is retained. Every restart pauses monitoring and trading. No real trading or signing is part of release verification. Updater signing is not Windows Authenticode signing.
