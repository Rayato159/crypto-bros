# Crypto Bros

Rust desktop workstation for SOL/USDC research, paper trading and **individually approved real trades**. Dioxus Web runs inside Tauri; the Rust worker keeps running independently of the window. SQLite keeps separate paper and real ledgers, models and feedback locally.

**0.3.0 adds a one-button trade watch, Windows notifications and a larger Thai live-account interface.** Every real Jupiter swap still requires your Phantom signature. Paper mode remains the default; real funding and limits must be configured explicitly. A +5% weekly return is a goal, not a guaranteed return or a reason to relax risk limits. Building and testing this app does not establish profitability or prove a user's mainnet execution.

## เฝ้าตลาดแบบไม่ต้องไล่กดหลายปุ่ม

หลังเชื่อม Phantom และบันทึกทุนใน **Live trading** กด **เริ่มเฝ้าตลาด** ครั้งเดียว แอปจะรีเฟรชข้อมูล เตรียมและเลือกสูตรเริ่มต้นหากยังไม่มี แล้วตรวจตลาดประมาณทุกนาที สูตรที่ใช้อยู่จะไม่ถูกแทนที่อัตโนมัติเมื่อมีผลวิจัยรอบใหม่

- กด **ทดสอบแจ้งเตือน** ก่อนซ่อนแอปไว้ในถาดระบบ คอมต้องเปิดและไม่หลับ Windows อาจซ่อนข้อความเมื่อปิดแจ้งเตือนหรือเปิดห้ามรบกวน
- เมื่อโมเดลผ่านเกณฑ์ซื้อ การ์ดจะแสดงจำนวนที่แบ่งจากวงเงินและจำนวนไม้ที่เหลือ โดยไม่ใช้เงินสำรองหรือกำไรที่ยังพักไว้
- เมื่อถึงราคาเป้าหมายหรือจุดทบทวนความเสี่ยง การ์ดจะเสนอให้ตรวจการขายไม้ที่เกี่ยวข้อง การขายทั้งหมดหมายถึงทั้งหมดของไม้นั้น ไม่ใช่ทุกเหรียญในกระเป๋า
- กดแจ้งเตือนเพื่อเปิดแอป แล้วกด **ตรวจราคาซื้อ → Phantom** หรือ **ตรวจราคาขาย → Phantom** ระบบจะตรวจสัญญาณ ราคา วงเงิน และธุรกรรมใหม่ ก่อนเปิดหน้าที่ให้มึงตรวจและเซ็นเอง หากสัญญาณหายหรือราคา/ต้นทุนไม่ผ่าน จะบอกเหตุผลและไม่เตรียมรายการ
- คำแจ้งเตือนเดิมเว้นอย่างน้อย 15 นาที ปุ่ม **เตือนอีกทีใน 15 นาที** พักเฉพาะสัญญาณนั้น การถึงจุดความเสี่ยงยังบล็อกการซื้อเพิ่ม
- **พักซื้อเพิ่ม** ยังเฝ้าเงื่อนไขขายอยู่ ส่วน **หยุดเฝ้าตลาด** หยุดแจ้งสัญญาณและพักซื้อเพิ่ม การหยุดทั้งสองแบบไม่ได้ขายเหรียญที่ถือให้เอง เปิดแอปใหม่ต้องกดเริ่มเฝ้าอีกครั้ง
- รอรายการเข้า **ประวัติซื้อขายที่เครือข่ายยืนยันแล้ว** หากยังค้าง ให้ตรวจผลคำสั่งเดิม อย่าส่งซ้ำในหน้าต่างอื่น

ตัวเลขในการ์ดใช้ราคาอ้างอิงและยังไม่ใช่ราคาที่รับประกัน โดยเฉพาะผลกำไร/ขาดทุนก่อนค่าขายจริง กดตรวจรายการแล้วจึงได้ราคาและค่าธรรมเนียมล่าสุด Windows รายงานการกดแจ้งเตือนผ่านปลั๊กอินได้ขณะ toast ยังแสดงอยู่; หากข้อความเก่าในศูนย์แจ้งเตือนไม่เปิดแอป ให้เปิด Crypto Bros จากถาดระบบแทน ([Tauri notification behavior](https://docs.rs/tauri-plugin-notification/latest/tauri_plugin_notification/struct.NotificationBuilder.html)).

## อ่านตัวเลขที่จำเป็น

| ตัวเลข | ความหมาย |
|---|---|
| มูลค่าทุนบอต · USDC | USDC ที่บันทึกให้บอต รวมมูลค่า SOL ที่ซื้อผ่านบอต ส่วน SOL เดิมหรือที่เติมจ่ายค่าเครือข่ายแสดงแยกและไม่รวมในทุนนี้ |
| ทุนบอตคิดเป็นเงินบาท | มูลค่าทุนบอตแปลงด้วยเรต USDC/THB อ้างอิง ยอดถอนเข้าธนาคารอาจต่างกัน |
| ผลกำไร/ขาดทุนจากบอต · USDC | ผลตั้งแต่เริ่มบอต รวมไม้ที่ยังถือและค่าเครือข่ายที่บันทึกแล้ว |
| ผลตั้งแต่เริ่มบอตเป็นบาท | รวมผลของเรตเงินด้วย จึงขยับได้แม้ยังไม่เทรด |
| ผลเทียบเงินต้นเดิม · บาท | เทียบกับต้นทุนเงินบาทที่แบ่งมาจากการซื้อเดิม รวมผลก่อนเริ่มบอตด้วย |
| เงินสำรอง | USDC ในกระเป๋าบอตที่กันไว้ไม่ใช้ซื้อ; ค่าเครือข่ายต้องมี SOL แยก |
| กำไรที่พักไว้ | กำไรที่ขายรับมาแล้วและยังไม่อนุญาตให้ลงทุนต่อ การปล่อยกำไรยังอยู่ภายใต้วงเงินเดิม |

ข้อความสถานะการกดปุ่มอยู่มุมล่างของหน้าจอเสมอ ปุ่มที่กำลังทำงานแสดงสถานะรอ หากทำไม่ได้จะแสดงเหตุผล ส่วนการเลือกสูตรเองและปุ่ม **ซื้อเองด้วยเงินจริง (ข้ามโมเดล)** อยู่ในเครื่องมือขั้นสูง ปุ่มนี้ไม่ใช่ paper trading

## Run the Windows app

Download the **x64 MSI** and `SHA256SUMS.txt` from [GitHub Releases](https://github.com/Rayato159/crypto-bros/releases), follow its setup wizard, then open **Crypto Bros** from Start. This preview is not Windows code-signed. Updates are manual; the signed in-app update channel is not configured. Before replacing an installed version, reconcile any signed pending order and choose **Stop bot and quit**. Preserve `%LOCALAPPDATA%\CryptoBros\data`; never delete it to fix an upgrade. Version 0.1 used NSIS and may retain a separate uninstall entry after installing MSI.

1. **Refresh market** loads public SOL/USDC 15-minute candles. Alternatively choose **Load synthetic demo** to explore without a network connection. The app labels synthetic data explicitly.
2. **สร้าง / ทดสอบสูตรใหม่** trains the initial model and searches 64 policy candidates. Inspect the holdout results under Research.
3. **Select for paper trading**, then **Start paper engine**. Selecting a model alone does not start trading. Manual paper layers are also available.
4. Inspect each layer's cost, target, review threshold and realized fills. Close part or all of a layer. Profits remain parked until **Release parked profit**.
5. Close the window to hide it in the tray. Use **Pause trading and new buys** or **Stop bot and quit** from the tray. Restart always pauses new entries; signed pending transactions remain queued for reconciliation. Sleep, shutdown or lost connectivity prevents monitoring.

Paper capital starts at 1,000 USDC: 400 protected reserve, four layers with budgets up to 150 USDC including fees. Source changes are blocked after any fill to prevent mixing synthetic and market results. Use a separate `--data-dir` for a separate paper account; preserve your existing data.

## Wallet: use Phantom together with Jupiter

Phantom holds the wallet and authorizes signatures. Jupiter finds swap routes and provides quotes. They serve different roles; there is no need to choose one to replace the other.

1. Download the browser extension from [phantom.com/download](https://phantom.com/download). For stronger separation, use a separate browser profile and create a **new trading wallet with its own recovery phrase**, leaving your personal wallet intact.
2. Follow [Phantom's official new-wallet guide](https://help.phantom.com/articles/45135465489555): Create a New Wallet → Create a Recovery Phrase Wallet. Set the password and privately back up the phrase offline. Do these steps yourself. Never send the phrase to the app, chat, logs, support, or `.env`.
3. In Crypto Bros → Wallet, choose **Open Phantom connection**. The short-lived page opens in your default browser; that browser/profile must have Phantom. It requests only a public address. If your trading profile differs from the default browser, enter its public address in **Watch a public Solana address** instead.
4. **Refresh on-chain balance** reads SOL and Solana USDC balances. These balances are separate from paper capital. Connecting an address is not a proof of ownership and does not grant spending rights.
5. No deposit is needed for paper trading, model research or quote previews. For real trading, fund only the separate bot wallet with Solana USDC and native SOL for costs, then follow the Live trading setup below. An additional account under the same recovery phrase separates balances, not recovery secrets.

**Do not put wallet seeds or private keys in `.env`.** The application has no raw-wallet-key configuration or export feature. Agent rules are in [AGENTS.md](AGENTS.md).

## Real trading and THB accounting

1. Connect the dedicated wallet, open **Live trading**, and refresh prices, FX and wallet. Synthetic prices cannot authorize real orders.
2. Enter the original THB purchase cost, total USDC received from that purchase, and USDC allocated to the bot. The app assigns `original THB × allocated USDC / total USDC` as historical cost, rounded to satang. Confirm separately whether the original amount includes all fees. Personal amounts are never bundled into releases.
3. Set the maximum active investment, maximum buy and unspent USDC reserve. The first release supports two open layers. Native SOL must separately cover tracked SOL positions, a 0.005 SOL fee buffer, the quoted network fee and any account rent. The buffer is not a per-transaction fee. Gasless and multisigner routes are intentionally unsupported.
4. Enable manually approved buys. Run research and explicitly select its candidate for live advice. **เตรียมซื้อเมื่อผ่านโมเดล** requires a qualifying signal and rechecks the executable quote's estimated round-trip costs. **ซื้อเองด้วยเงินจริง (ข้ามโมเดล)** bypasses only the signal, retaining every hard limit.
5. Review the prepared amounts, then open **ตรวจรายการและยืนยันใน Phantom**. Chrome is preferred, with the default browser as fallback. Compare the dedicated wallet, amount, minimum receipt and fees in Phantom. The approval link expires quickly; unlock Phantom before preparing a quote.
6. The worker saves the verified signature before submission. A timeout leaves the order pending. Only finalized chain balances create a fill; clicking twice, restarting, or losing a response cannot authorize a second order. Use **ตรวจผลคำสั่งเดิมอีกครั้ง** after an ambiguous result. Do not manually repeat a pending trade elsewhere.
7. Sell half or all of a tracked layer through the same approval flow. Profits stay parked until released, within the original caps. Targets and review prices are decision thresholds, not guaranteed execution prices.

The live dashboard separates original allocated THB cost, current marked value, bot USDC performance, and bot THB performance since onboarding. Earlier SOL/FX changes belong to historical P&L, not bot returns. USDC/THB comes from a Coinbase reference rate; it is not a guaranteed Bitkub withdrawal rate. Actual network fees are recorded in SOL and valued at the order's reference prices; account rent is recorded separately. Unconfirmed funding fees remain explicitly marked as incomplete. External deposits do not increase limits, and withdrawals below tracked balances block further preparation.

Live buys pause at a layer review price, a 10% loss against starting bot capital, or a 50% loss against allocated historical THB cost. You approve any sale; alerts never fabricate a stop-loss fill. The real account remains bound to its wallet and funding history. Use a separate data directory for another funded account.

## APIs and optional email

| Service | Current use | Cost / setup |
|---|---|---|
| Binance public market data | Completed SOLUSDC candles and ticker, once per minute | Public endpoint; no API key. Regional/network availability may vary. |
| Jupiter Swap V2 | Quotes, Metis legacy/v0 orders, and execution after a verified Phantom signature | Keyless access where supported; optional API key. Route and network costs still apply. |
| Solana RPC | Public balances, simulation, fee checks and finalized reconciliation | Default public RPC; rate limits apply. A private provider may be needed for availability/history. |
| Coinbase Data API | USDC/THB reference valuation | Public exchange-rate endpoint; records local fetch time rather than claiming a provider quote timestamp. |
| Resend | Optional risk review emails | Requires your API key, verified sender and chosen recipient. Not configured by default. |

Official references: [Binance market-data-only](https://developers.binance.com/en/docs/binance-spot-api-docs/faqs/market_data_only), [Jupiter pricing](https://developers.jup.ag/pricing), [Jupiter order/execute](https://developers.jup.ag/docs/swap/order-and-execute), [Phantom transaction signing](https://docs.phantom.com/solana/sending-a-transaction), [Coinbase reference rates](https://docs.cdp.coinbase.com/coinbase-app/track-apis/exchange-rates), [Resend email API](https://resend.com/docs/api-reference/emails/send-email). Provider pricing may change. Free data access does not make trading free: spread, slippage, network/priority fees and route fees still matter. Charts render locally; no paid chart subscription is required.

The worker privately loads supported service settings from `.env` **in its data directory**, not from the repository or executable directory. The default Windows location is `%LOCALAPPDATA%\CryptoBros\data`. Copy `.env.example` there yourself and fill optional fields privately. Restart the worker afterward. Never give credentials to a coding agent. The application restricts this directory to the current OS user and SYSTEM on Windows; `.env` remains plaintext. OS credential-store support is not implemented yet.

Risk review starts at any open layer's 3× ATR review price or 10% equity drawdown; a 50% total-capital loss is also detected. The engine pauses and retains open positions. You decide whether to sell. Acknowledging an alert does not bypass a hard limit. When configured, email retries at most three times; provider acceptance is not proof of inbox delivery. No email can be delivered while the computer is asleep/offline. The protected USDC allocation is a budgeting rule, not protection against USDC depeg or chain/custody risk.

## Model and evidence

The initial model is ridge regression, not deep learning or Q-learning. Inputs: RSI14, MACD histogram, four-bar momentum, ATR/price, volume ratio. It predicts four 15-minute bars ahead. Training-only scaling, later error calibration, chronological validation and an untouched holdout reduce leakage. The optimizer maximizes a validation score penalizing drawdown and unstable results across two periods; it cannot optimize reserve, maximum layers or hard drawdown limits.

Candidates are never promoted automatically. Initial public history is only about ten days, which is insufficient to establish robustness. Scheduled research attempts every 72 hours after the first run and requires an entirely new holdout. Feedback is stored for later supervised/reward experiments; **it does not currently retrain the policy**. See [model design](docs/MODEL.md).

## Build and development

Requires Windows x64, Rust with the `wasm32-unknown-unknown` target, MSVC build tools, Node 22 or newer with npm, Dioxus CLI 0.7.2 and WebView2. The installer can bootstrap WebView2. Commit `Cargo.lock` and `package-lock.json` for reproducibility.

```sh
npm ci
npm run test:wallet
cargo test --locked
cargo check -p crypto-bros-ui --target wasm32-unknown-unknown --locked
npm run build
# Optional read-only internet smoke check, never run by the default test suite:
cargo test --test public_data --locked -- --ignored --nocapture
```

CLI worker: `crypto-bros.exe serve`, `status`, `start`, `pause`, `stop`. Optional `--data-dir <folder>` separates accounts; worker `--port 0` chooses a free localhost port. The desktop also accepts `--data-dir <folder>` and chooses a free localhost port, so independent accounts do not collide. A single worker may own a data directory at a time. Do not inspect the private session file; use the CLI or app. The Live trading tab can archive a synthetic demo into a local SQLite backup before starting a fresh market account.

The portable Node preparation command builds the worker and web frontend; Tauri/WiX produces the MSI in `target/release/bundle/msi/`. Node also bundles the local Phantom approval script, so `npm ci` is required before compiling the worker. The approval page does not load a remote CDN script. Windows-specific local scripts are excluded from this repository. Desktop packaging on other platforms has not been verified.

Signed in-app update infrastructure is implemented but unconfigured in this preview. It still needs an HTTPS update manifest and publisher-controlled signing key. See [release instructions](docs/RELEASING.md) and [verification log](docs/DEV_LOG.md).
