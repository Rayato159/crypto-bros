mod chart;
mod live_details;
mod updates_ui;
use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

// A statically bundled bridge avoids eval/Function and keeps the CSP strict.
#[wasm_bindgen(
    inline_js = "export async function invoke_json(name, args) { try { if (!window.__TAURI__) throw new Error('Open the installed Crypto Bros app to connect to the engine.'); return JSON.stringify({ok:true,value:await window.__TAURI__.core.invoke(name,JSON.parse(args))}); } catch(e) { return JSON.stringify({ok:false,error:String(e)}); } } export function local_time(seconds) { return new Date(seconds*1000).toLocaleString(); }"
)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn invoke_json(name: &str, args: &str) -> Result<JsValue, JsValue>;
    fn local_time(seconds: f64) -> String;
}

#[derive(Clone, Copy)]
struct Context {
    state: Signal<Value>,
    busy: Signal<bool>,
    notice: Signal<String>,
}
fn main() {
    dioxus::launch(app);
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn money(v: f64) -> String {
    format!("{v:.2}")
}
fn pct(v: f64) -> String {
    format!("{:+.2}%", v * 100.0)
}
fn items(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
async fn invoke(name: &str, args: Value) -> Result<Value, String> {
    let raw = invoke_json(name, &args.to_string())
        .await
        .map_err(|_| "Desktop connection failed")?;
    let result: Value = serde_json::from_str(&raw.as_string().ok_or("Invalid desktop response")?)
        .map_err(|_| "Invalid desktop response")?;
    if result["ok"] == true {
        Ok(result["value"].clone())
    } else {
        Err(s(&result["error"]).into())
    }
}
async fn run_action(mut ctx: Context, action: String, payload: Value) {
    if (ctx.busy)() {
        return;
    }
    ctx.busy.set(true);
    ctx.notice.set(if action == "live_watch_start" {
        "กำลังตรวจข้อมูลและเตรียมสูตรเริ่มต้น โปรดรอสักครู่…".into()
    } else if action == "research" {
        "กำลังทดสอบสูตรกับข้อมูลย้อนหลัง โปรดรอสักครู่…".into()
    } else {
        "กำลังทำรายการ…".into()
    });
    match invoke("command", json!({"action":action,"payload":payload})).await {
        Ok(_) => {
            ctx.notice.set(
                match action.as_str() {
                    "enable_live" => "เปิดเสนอซื้อเพิ่มแล้ว ถ้าสัญญาณยังเป็น wait ระบบจะยังไม่เตรียมซื้อ",
                    "research" => "ทดสอบสูตรเสร็จแล้ว ดูผลใน Research หรือกดใช้สูตรที่ทดสอบล่าสุด",
                    "live_use_model" => "เลือกสูตรแนะนำแล้ว ระบบอาจยังให้รอหากโอกาสไม่ผ่านเกณฑ์",
                    "live_allocate_80_20" => {
                        "บันทึกทุนและสำรอง 20/80 แล้ว ยังไม่มีการซื้อขาย ตรวจตัวเลขแล้วเริ่มเฝ้าตลาดได้"
                    }
                    "live_watch_start" => "เริ่มเฝ้าตลาดแล้ว เมื่อมีสัญญาณจะขึ้นการ์ดและแจ้งเตือนให้ตรวจรายการ",
                    "live_watch_stop" => "หยุดเฝ้าตลาดและพักซื้อเพิ่มแล้ว เหรียญที่ถือยังอยู่ในกระเป๋า",
                    "live_watch_snooze" => "พักการแจ้งเตือนสัญญาณนี้ 15 นาทีแล้ว ไม่มีการซื้อขาย",
                    "live_prepare" => "เตรียมคำสั่งแล้ว ตรวจส่วนรายการรอยืนยันและกดเปิด Phantom",
                    "live_refresh" => "อัปเดตราคา เรตเงินบาท และยอดกระเป๋าแล้ว",
                    "live_pause" => "พักซื้อเพิ่มแล้ว ยังติดตามเงื่อนไขขายของไม้เดิมเมื่อเปิดเฝ้าตลาดอยู่",
                    _ => "ทำรายการเสร็จแล้ว ดูสถานะล่าสุดในหน้านี้",
                }
                .into(),
            );
            if let Ok(value) = invoke("snapshot", json!({})).await {
                ctx.state.set(value);
            }
        }
        Err(error) => ctx
            .notice
            .set(format!("ทำรายการไม่ได้: {}", friendly_error(&error))),
    }
    ctx.busy.set(false);
}
fn friendly_error(error: &str) -> String {
    if error.contains("Model recommends waiting") || error.contains("Model forecast does not clear")
    {
        "โมเดลยังแนะนำให้รอ โอกาสกำไรยังไม่ผ่านความไม่แน่นอนและต้นทุน จึงยังไม่เตรียมซื้อ".into()
    } else if error.contains("holdout") || error.contains("new candles") {
        "ยังมีแท่งราคาใหม่ไม่พอ ทดสอบสูตรรอบใหม่ต้องรอประมาณ 180 แท่ง 15 นาที (ราว 45 ชั่วโมง) เพื่อไม่ใช้ข้อมูลทดสอบซ้ำ หากสูตรเดิมผ่านเกณฑ์ยังใช้เฝ้าต่อได้".into()
    } else if error.contains("consume the model") {
        "ราคาจริงและค่าธรรมเนียมกินโอกาสกำไรที่โมเดลประเมินไว้ ระบบจึงไม่เตรียมซื้อ".into()
    } else {
        error.into()
    }
}
#[component]
fn Action(
    label: String,
    action: String,
    #[props(default=json!({}))] payload: Value,
    #[props(default=String::from("secondary"))] kind: String,
) -> Element {
    let ctx = use_context::<Context>();
    let mut active = use_signal(|| false);
    rsx! {button {class:"button {kind}",disabled:(ctx.busy)(),onclick:move |_|{
        let action=action.clone();let payload=payload.clone();
        spawn(async move {active.set(true);run_action(ctx,action,payload).await;active.set(false);});
    },if active(){"กำลังทำงาน…"}else{"{label}"}}}
}
#[component]
fn Metric(
    label: String,
    value: String,
    detail: String,
    #[props(default = false)] accent: bool,
) -> Element {
    rsx! {div {class:if accent{"metric accent"}else{"metric"},div {class:"eyebrow","{label}"}div {class:"metric-value","{value}"}div {class:"muted small","{detail}"}}}
}
fn app() -> Element {
    let state = use_signal(|| Value::Null);
    let busy = use_signal(|| false);
    let notice = use_signal(|| String::from("Connecting to your local engine…"));
    let ctx = use_context_provider(|| Context {
        state,
        busy,
        notice,
    });
    let mut tab = use_signal(|| String::from("Overview"));
    use_future(move || async move {
        let mut ctx = ctx;
        loop {
            match invoke("snapshot", json!({})).await {
                Ok(mut value) => {
                    if value["focus_live"] == true
                        || ((ctx.state)().is_null() && !value["real"].is_null())
                    {
                        tab.set("Live trading".into());
                    }
                    value["engine_connected"] = json!(true);
                    ctx.state.set(value);
                    if (ctx.notice)().starts_with("Connecting") {
                        ctx.notice
                            .set("Engine connected · real orders require Phantom approval".into());
                    }
                }
                Err(error) => {
                    ctx.state.with_mut(|v| {
                        v["engine_connected"] = json!(false);
                        v["trade_watch"]["signals"] = json!([]);
                    });
                    ctx.notice.set(format!(
                        "Engine disconnected · {error}. Displayed values may be stale."
                    ));
                }
            }
            gloo_timers::future::TimeoutFuture::new(2000).await;
        }
    });
    let data = state();
    let source = s(&data["source"]);
    let source_label = match source {
        "synthetic_demo" => "SYNTHETIC DEMO",
        "binance_solusdc" => "BINANCE · SOL/USDC",
        _ => "AWAITING MARKET DATA",
    };
    let running = data["running"] == true;
    rsx! {
        style {"{include_str!(\"style.css\")}"}
        style {"{include_str!(\"live.css\")}"}
        style {"{include_str!(\"readable.css\")}"}
        div {class:"shell",
            aside {class:"sidebar",div {class:"brand",div {class:"brand-mark","C↗"}div {class:"brand-name",div {"CRYPTO"}div {"BROS"}}}div {class:"workspace-label","PERSONAL TRADING LAB"}
                nav {for (index,name) in ["Overview","Live trading","Layers","Research","Decisions","Wallet","Settings"].into_iter().enumerate(){button {class:if tab()==name{"nav-item active"}else{"nav-item"},onclick:move |_|tab.set(name.into()),span {class:"nav-number","0{index+1}"}span {"{name}"}if tab()==name{span {class:"nav-arrow","↗"}}}}}
                div {class:"sidebar-bottom",div {class:"status-line",span {class:if running{"dot live"}else{"dot"}}if running{"PAPER ENGINE RUNNING"}else{"PAPER ENGINE PAUSED"}}p {"Your machine. Your data."}div {class:"version","v0.5.1 · MANUAL APPROVAL"}}
            }
            main {class:"main",header {class:"topbar",div {class:"breadcrumb","WORKSTATION",span {"/"},strong {"{tab()}"}}div {class:"topbar-right",span {class:"source-tag","{source_label}"}span {class:"paper-badge",if tab()=="Live trading"{"REAL MONEY · PHANTOM"}else if tab()=="Overview"||tab()=="Layers"{"PAPER TRADING"}else{"RESEARCH / ACCOUNT TOOLS"}}}}
                div {class:"content",
                    if source=="synthetic_demo"{div {class:"demo-banner","SYNTHETIC DATA · Prices, fills and results are simulated. They are not market performance."}}
                    if data["market_error"].is_string(){div {class:"warning","{s(&data[\"market_error\"])}"}}
                    match tab().as_str(){"Live trading"=>rsx!{LiveTrading{}},"Layers"=>rsx!{Layers{}},"Research"=>rsx!{Research{}},"Decisions"=>rsx!{Decisions{}},"Wallet"=>rsx!{Wallet{}},"Settings"=>rsx!{Settings{}},_=>rsx!{Overview{}}}
                    div {class:"notice",span {class:"notice-label",if busy(){"WORKING"}else{"ACTIVITY"}}span {"{notice()}"}}
                    footer {span {"LOCAL FIRST / SOLANA"}span {"เป้ากำไรต่อไม้ตามสภาพตลาด · ไม่มีขั้นต่ำรายวัน · ทุกคำสั่งจริงยืนยันใน Phantom"}}
                }
            }
        }
    }
}
#[component]
fn Overview() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let ledger = &data["ledger"];
    let running = data["running"] == true;
    let cash = n(&ledger["cash"]) / 1e6;
    let reserve = n(&ledger["reserve"]) / 1e6;
    let parked = n(&ledger["parked_profit"]) / 1e6;
    let equity = if data.is_null() {
        1000.0
    } else {
        n(&data["equity"])
    };
    let open = items(&ledger["layers"])
        .iter()
        .filter(|l| n(&l["quantity"]) > 0.0)
        .count();
    let indicators = &data["indicators"];
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","THE LONG GAME"}h1 {"A clear view."}p {"Observe the market. Test the edge. Protect the reserve."}}div {class:"actions",Action {label:"↻ Refresh market",action:"refresh"}Action {label:if running{"Pause engine"}else{"Start paper engine"},action:if running{"pause"}else{"start"},kind:"primary"}}}
        div {class:"metrics",Metric {label:"PAPER EQUITY",value:format!("{} USDC",money(equity)),detail:format!("{} total return · includes open positions",pct(n(&data["net_return"]))),accent:true}Metric {label:"PROTECTED RESERVE",value:money(if data.is_null(){400.0}else{reserve}),detail:"USDC · excluded from trading budget"}Metric {label:"AVAILABLE TO DEPLOY",value:money((cash-reserve-parked).max(0.0)),detail:format!("{open} / 4 layers active")}Metric {label:"DRAWDOWN",value:format!("{:.2}%",n(&data["drawdown"])*100.0),detail:"From paper equity high-water mark"}}
        div {class:"overview-grid",
            section {class:"panel chart-panel",div {class:"panel-heading",div {span {class:"pair-icon","◎"}strong {"SOL / USDC"}span {class:"muted","15m · closed candles"}}div {class:"chart-price",if n(&data["price"])>0.0{"{money(n(&data[\"price\"]))}"}else{"—"}}}
                div {class:"chart",chart::CandleChart{candles:data["candles"].clone()}}
                div {class:"indicator-row",div {span {"RSI · 14"}strong {"{n(&indicators[\"rsi\"]):.1}"}}div {span {"MACD HISTOGRAM"}strong {"{n(&indicators[\"histogram\"]):.4}"}}div {span {"ATR · 14"}strong {"{n(&indicators[\"atr\"]):.3}"}}}
                div {class:"panel-note","Chart source and swap quote are different markets. Paper fills include simulated fees and slippage."}
            }
            section {class:"panel allocation",div {class:"eyebrow","CAPITAL DISCIPLINE"}h2 {"Every dollar\nhas a job."}div {class:"allocation-bar",span {class:"reserve-segment"}span {class:"trading-segment"}}
                div {class:"allocation-line",span {class:"legend-square reserve"}span {"Reserve"}strong {"40%"}}div {class:"allocation-line",span {class:"legend-square trading"}span {"Trading budget"}strong {"60%"}}
                div {class:"divider"}div {class:"allocation-line",span {"กำไรที่ขายรับมาแล้วและพักไว้"}strong {"{money(parked)}"}}p {class:"muted small","Profit stays parked until you release it. The optimizer cannot access the reserve."}Action {label:"Release parked profit",action:"reinvest"}
            }
        }
        div {class:"bottom-grid",
            section {class:"panel",div {class:"panel-heading",h3 {"Next move"}span {class:"chip","MODEL + RISK CHECK"}}
                if data["active_model"].is_null(){div {class:"empty",h3 {"An edge needs evidence."}p {"Load candles, run research, then select a model for paper trading. Real accounts have separate controls under Live trading."}}}else{div {class:"model-summary",span {class:"big-label","WAIT / BUY / SELL"}p {"Active: {s(&data[\"active_model\"][\"version\"])}"}p {class:"muted","A forecast must clear uncertainty, costs and risk limits."}}}
                div {class:"actions",Action {label:"สร้าง / ทดสอบสูตรใหม่",action:"research"}Action {label:"Open paper layer",action:"buy"}}
            }
            section {class:"panel",div {class:"panel-heading",h3 {"System watch"}span {class:"chip","LOCAL ENGINE"}}
                div {class:"watch-row",span {"Real-money orders"}strong {"SEPARATE LIVE TAB"}}div {class:"watch-row",span {"Email risk alerts"}strong {if data["email_configured"]==true{"CONFIGURED"}else{"NOT CONFIGURED"}}}div {class:"watch-row",span {"เป้าหมาย"}strong {"สุทธิต่อไม้ / ไม่บังคับรายวัน"}}
                for alert in items(&data["alerts"]).into_iter().filter(|a|a["acknowledged"]!=true){div {class:"warning small","{s(&alert[\"message\"])}"}}
                Action {label:"Acknowledge risk review",action:"acknowledge"}
            }
        }
        if data["source"]=="not_loaded" || data.is_null(){div {class:"demo-callout",span {"Explore without a data connection"}Action {label:"Load synthetic demo",action:"demo"}}}
        if data["source"]=="synthetic_demo"{div {class:"demo-callout",span {"Advance the synthetic replay by one 15-minute candle."}Action {label:"Step replay →",action:"demo_step"}}}
    }
}
#[component]
fn Layers() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let rows = items(&data["ledger"]["layers"]);
    let price = n(&data["price"]);
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","POSITION ACCOUNTING"}h1 {"One layer at a time."}p {"Simulated fills, allocated cost basis and realized profit."}}Action {label:"+ Open paper layer",action:"buy",kind:"primary"}}
        section {class:"panel",div {class:"panel-heading",h3 {"Layer ledger"}span {class:"chip","COST BASIS PER LAYER"}}
            if rows.is_empty(){div {class:"empty",h2 {"No positions. No pressure."}p {"A paper layer uses up to 150 USDC including simulated fees. Load data before opening one."}}}else{
                table {thead {tr {th {"LAYER"}th {"SOL REMAINING"}th {"COST / USDC"}th {"UNREALIZED"}th {"TARGET / REVIEW"}th {"ACTION"}}}tbody {for layer in rows {tr {
                    td {strong {"#{n(&layer[\"id\"]) as u64:03}"}div {class:"muted small","{s(&layer[\"model_version\"])}"}}td {"{n(&layer[\"quantity\"])/1e9:.6}"}td {"{money(n(&layer[\"remaining_cost\"])/1e6)}"}
                    td {class:if n(&layer["quantity"])/1e9*price-n(&layer["remaining_cost"])/1e6>=0.0{"positive"}else{"negative"},"{money(n(&layer[\"quantity\"])/1e9*price-n(&layer[\"remaining_cost\"])/1e6)}"}
                    td {"{money(n(&layer[\"target_price\"])/1e6)} / {money(n(&layer[\"review_price\"])/1e6)}"}
                    td {if n(&layer["quantity"])>0.0{div {class:"actions compact",Action {label:"ขายครึ่งไม้",action:"sell",payload:json!({"id":layer["id"],"percent":50})}Action {label:"Close",action:"sell",payload:json!({"id":layer["id"],"percent":100})}}}else{span {class:"muted","CLOSED"}}}
                }}}}
            }
        }
        section {class:"panel spaced",div {class:"panel-heading",h3 {"Execution journal"}span {class:"chip","PAPER FILLS ONLY"}}
            table {thead {tr {th {"FILL"}th {"SIDE"}th {"SOL"}th {"PRICE"}th {"FEE"}th {"REALIZED P&L"}}}tbody {for fill in items(&data["ledger"]["fills"]).into_iter().rev().take(30){tr {td {"#{n(&fill[\"id\"]) as u64}"}td {"{s(&fill[\"side\"])}"}td {"{n(&fill[\"quantity\"])/1e9:.6}"}td {"{money(n(&fill[\"price\"])/1e6)}"}td {"{money(n(&fill[\"fee\"])/1e6)}"}td {"{money(n(&fill[\"pnl\"])/1e6)}"}}}}}
        }
    }
}
#[component]
fn Research() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let r = &data["research"];
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","EVIDENCE BEFORE EXECUTION"}h1 {"Find an edge.\nThen challenge it."}p {"Ridge regression · one-hour horizon · bounded parameter search"}}Action {label:"Run 64-trial search",action:"research",kind:"primary"}}
        section {class:"panel",div {class:"panel-heading",h3 {"Chronological evaluation"}span {class:"chip","NO SHUFFLED SPLITS"}}
            div {class:"split-bar",div {class:"train","55% TRAIN"}div {class:"calibrate","10% CALIBRATE"}div {class:"validate","17% VALIDATE"}div {class:"holdout","18% HOLDOUT"}}
            p {class:"muted","Weights and scaling use training only. Four-candle gaps separate blocks. The final holdout never selects a trial."}
            if r.is_null(){div {class:"empty",h3 {"Your first experiment starts here."}p {"Load at least 600 completed candles. This initial sample is exploratory, not proof of a profitable strategy."}}}else{
                div {class:"metrics research-metrics",Metric {label:"HOLDOUT RETURN",value:pct(n(&r["holdout"]["net_return"])),detail:"Net costs; open positions marked"}Metric {label:"MAX DRAWDOWN",value:format!("{:.2}%",n(&r["holdout"]["max_drawdown"])*100.0),detail:"Holdout equity loss from peak"}Metric {label:"HOLDOUT FILLS",value:format!("{}",n(&r["holdout"]["trades"]) as u64),detail:"Few fills mean weak evidence"}Metric {label:"DEFAULT POLICY",value:pct(n(&r["baseline"]["net_return"])),detail:"Untuned policy benchmark"}}
                div {class:"research-status",span {class:"chip","{s(&r[\"status\"])}"}span {class:"muted","{s(&r[\"version\"])}"}Action {label:"Select for paper trading",action:"activate_paper"}}
                p {class:"muted","ชุดทดสอบแยกมีการเปิดไม้ซื้อ {n(&r[\"holdout\"][\"entries\"]) as usize} ไม้ · ต้องมีอย่างน้อย 8 ไม้ ผลตอบแทนสุทธิเป็นบวก และขาดทุนจากจุดสูงสุดต่ำกว่า 10% จึงเสนอใช้กับเงินจริงได้ การแบ่งขายหลายครั้งไม่นับเป็นไม้ซื้อเพิ่ม"}
            }
        }
        div {class:"bottom-grid spaced",
            section {class:"panel",div {class:"eyebrow","FORECAST MODEL"}h2 {"Simple enough\nto inspect."}p {"Standardized features predict the next four candles. Uncertainty is out-of-training prediction error."}
                for (i,name) in ["Intercept","RSI","MACD histogram","Momentum","ATR / price","Volume"].into_iter().enumerate(){div {class:"watch-row",span {"{name}"}strong {"{n(&r[\"model\"][\"weights\"][i]):.6}"}}}
            }
            section {class:"panel",div {class:"eyebrow","POLICY PARAMETERS"}h2 {"Bounded by design."}
                div {class:"watch-row",span {"Minimum net edge"}strong {"{pct(n(&r[\"policy\"][\"min_edge\"]))}"}}div {class:"watch-row",span {"Take-profit distance"}strong {"{n(&r[\"policy\"][\"take_profit_atr\"]):.2} × ATR"}}div {class:"watch-row",span {"Review distance"}strong {"3.00 × ATR"}}div {class:"watch-row",span {"Simulated fee / slippage per side"}strong {"20 / 10 bps"}}
                p {class:"muted small","Review holds the position and pauses entries. No guaranteed stop-loss fills. Candidates never replace the active model automatically."}p {class:"muted small","Another search requires 180 new candles. Scheduled research checks every 72 hours after the first run."}
            }
        }
    }
}
#[component]
fn Decisions() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    let rows = items(&data["decisions"]);
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","HUMAN IN THE LOOP"}h1 {"Show your work."}p {"Review the decision, not just whether the trade happened to win."}}span {class:"chip","{items(&data[\"feedback\"]).len()} FEEDBACK RECORDS"}}
        if rows.is_empty(){section {class:"panel empty",h2 {"The journal is waiting."}p {"Select a paper model and start the engine, or open a manual paper layer."}}}
        for d in rows.into_iter().rev().take(40){section {class:"panel decision-card",div {class:"decision-header",span {class:"decision-action","{s(&d[\"action\"])}"}span {class:"muted small","DECISION #{n(&d[\"id\"]) as u64} · {s(&d[\"model\"])}"}}p {"{s(&d[\"reason\"])}"}div {class:"decision-meta",span {"Forecast {pct(n(&d[\"forecast\"]))}"}span {"Net edge {pct(n(&d[\"net_edge\"]))}"}}div {class:"actions compact",for (rating,label) in [("good","Good decision"),("too_early","Too early"),("too_risky","Too risky"),("bad","Bad decision")]{Action {label,action:"feedback",payload:json!({"decision_id":d["id"],"rating":rating})}}}}}
    }
}
#[component]
fn Wallet() -> Element {
    let mut ctx = use_context::<Context>();
    let data = (ctx.state)();
    let mut address = use_signal(String::new);
    let wallet = &data["wallet"];
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","JUPITER QUOTES / PHANTOM WALLET"}h1 {"Keep the keys\nwhere they belong."}p {"Your wallet signs. Your workstation observes. No recovery phrase required."}}}
        div {class:"bottom-grid",
            section {class:"panel wallet-card",div {class:"wallet-symbol","◎"}h2 {"Connect Phantom"}p {"This connection step requests only your public address. A real trade uses a separate, expiring approval page and an explicit Phantom signature."}
                button {class:"button primary",onclick:move |_|{spawn(async move {match invoke("connect_wallet",json!({})).await{Ok(_)=>ctx.notice.set("Phantom connection opened in your browser. No signing request.".into()),Err(e)=>ctx.notice.set(e)}});},"Open Phantom connection ↗"}
                div {class:"divider"}label {class:"eyebrow","OR WATCH A PUBLIC SOLANA ADDRESS"}input {placeholder:"Public address only — never a seed or key",value:"{address()}",oninput:move |event|address.set(event.value())}Action {label:"Save public address",action:"wallet_address",payload:json!({"address":address()})}
            }
            section {class:"panel",div {class:"eyebrow","CONNECTED ACCOUNT"}h2 {"Read-only balance"}p {class:"wallet-address",if wallet.is_null(){"No wallet connected"}else{"{s(&wallet[\"address\"])}"}}
                div {class:"watch-row",span {"SOL"}strong {"{n(&wallet[\"sol\"]):.6}"}}div {class:"watch-row",span {"USDC on Solana"}strong {"{money(n(&wallet[\"usdc\"])):}"}}
                p {class:"muted small",if wallet["verified_at"].is_null(){"Balances have not been verified on-chain."}else{"Read from Solana RPC. Separate from your paper portfolio."}}Action {label:"Refresh on-chain balance",action:"balance"}
                div {class:"divider"}h3 {"Jupiter quote preview"}p {"Preview 150 USDC → SOL. No transaction is built or submitted."}
                if !data["quote"].is_null(){div {class:"watch-row",span {"Estimated SOL"}strong {"{n(&data[\"quote\"][\"sol_out\"]):.6}"}}div {class:"watch-row",span {"Effective USDC / SOL"}strong {"{money(n(&data[\"quote\"][\"effective_price\"]))}"}}}Action {label:"Get Jupiter quote",action:"quote"}
            }
        }
        section {class:"panel spaced",h3 {"Before real money"}p {"Create a separate Phantom trading wallet. Keep reserves in a wallet the app cannot spend from. No deposit is needed for paper trading, research or quotes."}p {class:"muted small","A second account under the same recovery phrase separates balances, not the recovery secret. A separate wallet/recovery setup provides stronger separation. Never send a seed or private key to this app, chat, or support."}}
    }
}
#[component]
fn LiveField(label: String, mut value: Signal<String>) -> Element {
    rsx! {label {class:"live-field",span {"{label}"}input {r#type:"text",inputmode:"decimal",value:"{value()}",oninput:move |event|value.set(event.value())}}}
}
#[component]
fn TradeWatch() -> Element {
    let mut ctx = use_context::<Context>();
    let data = (ctx.state)();
    let watch = &data["trade_watch"];
    let enabled = watch["enabled"] == true;
    let connected = data["engine_connected"] != false;
    rsx! {
        section {class:"panel spaced trade-watch",
            div {class:"panel-heading",h2 {"เฝ้าตลาดให้ แล้วมึงเลือกยืนยัน"}span {class:"chip",if enabled{"กำลังเฝ้า"}else{"ยังไม่เริ่ม"}}}
            p {class:"watch-copy","ตั้งทุนครั้งเดียวแล้วกดเริ่ม แอปเตรียมสูตรเริ่มต้นและตรวจตลาดทุกประมาณ 1 นาที เมื่อเข้าเกณฑ์ซื้อหรือขายจะแจ้งเตือนให้กดตรวจรายการ ไม่ต้องกรอกจำนวนเองทุกไม้"}
            div {class:"actions",
                if enabled {Action {label:"หยุดเฝ้าตลาด",action:"live_watch_stop"}}
                else {Action {label:"เริ่มเฝ้าตลาด",action:"live_watch_start",kind:"primary"}}
                if enabled && data["live_enabled"]!=true && data["real_advice"]["model_eligible"]==true && data["real_advice"]["risk_blocked"]!=true && data["real"]["pending"].is_null(){Action {label:"กลับมาเฝ้าทั้งซื้อและขาย",action:"live_watch_start",kind:"primary"}}
                button {class:"button secondary",onclick:move |_|{spawn(async move {
                    ctx.notice.set(match invoke("test_notification",json!({})).await {Ok(_)=>"ส่งคำขอแจ้งเตือนทดสอบแล้ว ถ้าเห็นข้อความ กดเพื่อกลับหน้านี้ ไม่มีการซื้อขาย".into(),Err(e)=>e});
                });},"ทดสอบแจ้งเตือน"}
            }
            p {class:"muted small","{s(&data[\"notification_status\"])}"}
            if enabled && data["real_advice"]["model_eligible"]==false {
                p {class:"warning","ยังเฝ้าเป้าขายและความเสี่ยงของไม้เดิม แต่สูตรยังใช้เสนอซื้อไม่ได้ ต้องเลือกสูตรที่ผ่านเกณฑ์ก่อน"}
            }
            p {class:"watch-copy","วงเงินสูงสุดต่อไม้ {money(n(&data[\"real\"][\"config\"][\"max_buy\"])/1e6)} USDC ระบบจะลดจำนวนลงตามเงินที่ใช้ได้และจำนวนไม้ที่เหลือ"}
            if !connected {p {class:"warning","ขาดการเชื่อมต่อกับตัวเฝ้าตลาด ข้อมูลอาจเก่า กรุณาเปิดแอปใหม่ก่อนทำรายการ"}}
            else if items(&watch["signals"]).is_empty(){div {class:"watch-empty","{s(&watch[\"message\"])}"}}
            div {class:"watch-signals",for signal in items(&watch["signals"]) {
                {
                    let key=s(&signal["key"]).to_owned();
                    let skip_key=key.clone();
                    let is_buy=signal["side"]=="buy";
                    rsx!{article {class:if signal["risk"]==true{"signal-card risk"}else if signal["below_cost_basis"]==true{"signal-card cost"}else{"signal-card"},
                        h3 {"{s(&signal[\"title\"])}"}
                        p {class:"signal-reason","{s(&signal[\"reason\"])}"}
                        p {class:"signal-money",if is_buy {"ใช้ {money(n(&signal[\"amount_usdc\"])/1e6)} USDC · ประมาณ ฿{money(n(&signal[\"value_satang\"])/100.0)}"}
                            else {"เสนอขาย SOL ของไม้ #{n(&signal[\"layer_id\"]) as u64} จำนวน {n(&signal[\"quantity_sol\"])/1e9:.9} SOL (ไม่แตะส่วนสำรอง)"}}
                        if !is_buy {p {"ผลกำไร/ขาดทุน ณ ราคาอ้างอิง: {money(n(&signal[\"pnl_usdc\"])/1e6)} USDC / ฿{money(n(&signal[\"pnl_satang\"])/100.0)}"}}
                        if signal["below_cost_basis"]==true {p {class:"cost-warning","ถึงเป้าราคาไม่ได้แปลว่ากำไรทั้งสองสกุล ตอนนี้ยังไม่คุ้มต้นทุน USDC หรือบาท ต้องตรวจราคาจริงและค่าขายก่อน หากกำไรไม่ครบทั้งสองสกุล ระบบจะยังไม่เตรียมขายทำกำไร"}}
                        p {class:"signal-estimate","ตัวเลขยังเป็นประมาณการก่อนค่าใช้จ่ายขาออก กดแล้วจะตรวจราคาจริงใหม่ หากสัญญาณเปลี่ยนจะไม่เตรียมคำสั่ง"}
                        div {class:"actions",
                            button {class:"button primary",disabled:(ctx.busy)()||!connected,onclick:move |_|{
                                let key=key.clone();
                                spawn(async move {
                                    if (ctx.busy)(){return;}
                                    ctx.busy.set(true);ctx.notice.set("กำลังตรวจสัญญาณ ราคา และวงเงินล่าสุด…".into());
                                    ctx.notice.set(match invoke("review_watch_order",json!({"key":key})).await {
                                        Ok(_)=>"เปิดหน้าตรวจรายการแล้ว ตรวจยอดและยืนยันใน Phantom ด้วยตัวเอง".into(),Err(e)=>e});
                                    if let Ok(value)=invoke("snapshot",json!({})).await{ctx.state.set(value);}
                                    ctx.busy.set(false);
                                });
                            },"{s(&signal[\"button\"])}"}
                            Action {label:"เตือนอีกทีใน 15 นาที",action:"live_watch_snooze",payload:json!({"key":skip_key})}
                        }
                    }}
                }
            }}
            p {class:"muted small","การกดแจ้งเตือนเปิดแอปเท่านั้น ทุกการซื้อและขายต้องเซ็นใน Phantom • คอมต้องเปิดและไม่หลับ • Windows อาจซ่อนแจ้งเตือนเมื่อเปิดห้ามรบกวน"}
        }
    }
}
#[component]
fn LiveTrading() -> Element {
    let mut ctx = use_context::<Context>();
    let data = (ctx.state)();
    let account = &data["real"];
    let advice = &data["real_advice"];
    let source_thb = use_signal(String::new);
    let source_usdc = use_signal(String::new);
    let received = use_signal(String::new);
    let budget = use_signal(|| "20".to_string());
    let max_buy = use_signal(|| "10".to_string());
    let reserve = use_signal(String::new);
    let amount = use_signal(|| "2".to_string());
    let mut fees_included = use_signal(|| false);
    let basis = source_thb().parse::<f64>().unwrap_or(0.0)
        * received().parse::<f64>().unwrap_or(0.0)
        / source_usdc().parse::<f64>().unwrap_or(0.0);
    let pending = &account["pending"];
    let pending_buy = pending["side"] == "buy";
    let spend_amount = n(&pending["input"]) / if pending_buy { 1e6 } else { 1e9 };
    let receive_amount = n(&pending["minimum_output"]) / if pending_buy { 1e9 } else { 1e6 };
    let spend_unit = if pending_buy { "USDC" } else { "SOL" };
    let receive_unit = if pending_buy { "SOL" } else { "USDC" };
    let metric = |field: &str, scale: f64, prefix: &str| {
        if advice[field].is_number() {
            format!("{prefix}{}", money(n(&advice[field]) / scale))
        } else {
            "—".into()
        }
    };
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","REAL WALLET · YOUR SIGNATURE"}h1 {"เฝ้าให้ มึงยืนยันเอง"}p {"USDC ↔ SOL · แบ่งได้ 2 ไม้ · ยืนยันซื้อขายใน Phantom ทุกครั้ง"}}div {class:"actions",Action {label:"รีเฟรชราคา เรตบาท และยอดกระเป๋า",action:"live_refresh"}Action {label:"พักซื้อเพิ่ม",action:"live_pause"}}}
        if data["wallet"].is_null(){section {class:"panel",h2 {"1. Connect your dedicated bot wallet"}p {"Select the wallet funded for this bot in Phantom. Keep your main reserve in your other wallet."}
            button {class:"button primary",disabled:(ctx.busy)(),onclick:move |_|{spawn(async move{match invoke("connect_wallet",json!({})).await{Ok(_)=>ctx.notice.set("Choose the bot wallet in Phantom, then return here.".into()),Err(error)=>ctx.notice.set(error)}});},"Connect Phantom in browser ↗"}
        }}else{p {class:"wallet-address muted small","Bot wallet: {s(&data[\"wallet\"][\"address\"])}"}}
        if account.is_null(){
            if data["source"]=="synthetic_demo"{section {class:"panel spaced",h2 {"Move from demo to real market data"}p {"Save the complete synthetic account as a local database backup, then start a fresh paper account. Your connected public wallet is kept."}Action {label:"Archive demo and start fresh",action:"archive_demo"}}}
            section {class:"panel spaced",h2 {"2. Record funding and original baht cost"}p {"Refresh the market and wallet first. If only part of a larger USDC holding was transferred here, the app allocates its original baht cost proportionally."}
                div {class:"live-form-grid",LiveField {label:"Original purchase cost · THB",value:source_thb}LiveField {label:"Total USDC obtained from that purchase",value:source_usdc}LiveField {label:"USDC actually received in this bot wallet",value:received}LiveField {label:"Maximum active investment · USDC",value:budget}LiveField {label:"Maximum per buy · USDC",value:max_buy}LiveField {label:"USDC reserve kept unspent here",value:reserve}}
                label {class:"live-check",input {r#type:"checkbox",checked:fees_included(),onchange:move |event|fees_included.set(event.checked())}span {"The original baht cost includes all purchase and transfer fees"}}
                if basis.is_finite()&&basis>0.0{p {"Allocated original cost: ฿{money(basis)}. This is separate from the wallet's current market value."}}
                p {class:"muted small","Hard limits: 2 open layers, 0.5% maximum quote slippage, 0.001 SOL maximum network fee per order. Keep at least 0.005 SOL untouched for future fees, plus enough SOL for this order and any token-account rent. Wallet keys stay in Phantom."}
                Action {label:"Record funding · keep trading paused",action:"live_setup",kind:"primary",payload:json!({"source_thb":source_thb(),"source_usdc":source_usdc(),"received_usdc":received(),"budget_usdc":budget(),"max_buy_usdc":max_buy(),"reserve_usdc":reserve(),"fees_included":fees_included()})}
            }
        }else{
            TradeWatch{}
            live_details::LiveDetails{}
            live_details::AllocationSetup{}
            div {class:"metrics",Metric {label:"มูลค่าทุนบอต · USDC",value:metric("equity_usdc",1e6,""),detail:"USDC ในบัญชีบอต + มูลค่า SOL ที่บอตซื้อไว้",accent:true}Metric {label:"ทุนบอตคิดเป็นเงินบาท",value:metric("equity_satang",100.0,"฿"),detail:"คำนวณตามเรตอ้างอิง ยังไม่ใช่ยอดถอนเข้าธนาคาร"}Metric {label:"ผลกำไร / ขาดทุนจากบอต · USDC",value:metric("bot_pnl_usdc",1e6,""),detail:"ตั้งแต่เริ่มใช้บอต หลังหักค่าเครือข่ายที่บันทึกแล้ว"}Metric {label:"ผลเทียบเงินต้นเดิม · บาท",value:metric("historical_pnl_satang",100.0,"฿"),detail:"รวมผลราคาและเรตเงินตั้งแต่ก่อนเริ่มใช้บอต"}}
            section {class:"panel spaced",div {class:"panel-heading",h2 {"ทุน เงินสำรอง และค่าเงินบาท"}span {class:"chip",if data["live_enabled"]==true{"เปิดเสนอซื้อ · มึงเซ็นทุกไม้"}else{"พักการซื้อเพิ่ม"}}}
                div {class:"watch-row",span {"เงินต้นเดิมที่แบ่งให้บอต (บาท)"}strong {"฿{money(n(&account[\"initial_cost_satang\"])/100.0)}"}}
                div {class:"watch-row",span {"ผลตั้งแต่เริ่มบอต (บาท รวมผลของเรตเงิน)"}strong {"{metric(\"bot_pnl_satang\",100.0,\"฿\")}"}}
                div {class:"watch-row",span {"USDC ตามบัญชีบอต / ส่วนที่กันไว้ไม่ใช้"}strong {"{n(&account[\"cash\"])/1e6:.6} / {n(&account[\"config\"][\"reserve\"])/1e6:.6}"}}
                div {class:"watch-row",span {"เพดานเงินลงทุน / สูงสุดต่อไม้"}strong {"{money(n(&account[\"config\"][\"trading_budget\"])/1e6)} / {money(n(&account[\"config\"][\"max_buy\"])/1e6)} USDC"}}
                div {class:"watch-row",span {"ยอดจริงในกระเป๋า: USDC / SOL (รวม SOL ที่เติมแยก)"}strong {"{n(&data[\"real_balances\"][\"usdc\"])/1e6:.6} / {n(&data[\"real_balances\"][\"sol\"])/1e9:.6}"}}
                div {class:"watch-row",span {"เรตอ้างอิง: บาทต่อ 1 USDC"}strong {"฿{n(&data[\"thb_rate\"][\"micros\"])/1e6:.4}"}}
                p {class:"muted small","เรตจาก Coinbase ใช้ประเมินมูลค่า ยอดถอนจริงอาจต่างกัน ทุนบอตไม่รวม SOL ที่มีอยู่เดิมหรือเติมไว้จ่ายค่าธรรมเนียม การเติมเงินเพิ่มไม่ได้เพิ่มวงเงินบอตอัตโนมัติ"}
                if account["funding_fees_included"]!=true{p {class:"warning small","ยังไม่ได้ยืนยันว่าเงินต้นรวมค่าซื้อและโอนครบ ผลเทียบเงินต้นเดิมอาจยังไม่รวมค่าใช้จ่ายเหล่านี้"}}
                div {class:"watch-row",span {"กำไรที่ขายรับมาแล้วและพักไว้"}strong {"{money(n(&account[\"parked_profit\"])/1e6)} USDC"}}Action {label:"นำกำไรกลับมาใช้ ภายในวงเงินเดิม",action:"live_reinvest"}
            }
            details {class:"panel spaced read-guide",summary {"วิธีอ่านตัวเลขในหน้านี้"} p {"มูลค่าทุนบอต: นับ USDC ที่บันทึกให้บอต และ SOL ที่ซื้อผ่านบอต ยังไม่รวม SOL เดิมที่เติมไว้จ่ายค่าเครือข่าย"} p {"ผลจากบอตเป็น USDC: ใช้ดูว่าตั้งแต่เริ่มบอต เงินเพิ่มหรือลดเท่าไร รวมกำไร/ขาดทุนของไม้ที่ยังถือและค่าเครือข่ายที่บันทึกแล้ว"} p {"ผลจากบอตเป็นบาท: รวมผลของเรต USDC/THB ด้วย จึงขยับได้แม้ยังไม่ได้ซื้อขาย"} p {"ผลเทียบเงินต้นเดิม: เทียบกับต้นทุนเงินบาทที่แบ่งมาจากการซื้อเดิม รวมผลก่อนเริ่มบอตด้วย ตัวเลขติดลบตรงนี้อย่างเดียวไม่ได้แปลว่าบอตเทรดขาดทุน"} p {"เงินสำรอง: เงินในกระเป๋าบอตที่แอปกันไม่ให้ใช้ซื้อ กำไรที่พักไว้จะไม่ถูกนำมาซื้อจนกดอนุญาต และยังอยู่ในวงเงินเดิม"}}
            if pending.is_null(){
                details {class:"advanced-tools",summary {"เครื่องมือขั้นสูง: ดูสูตร หรือเตรียมคำสั่งเอง"}
                section {class:"panel spaced",div {class:"panel-heading",h2 {"Next move: {s(&advice[\"action\"])}"}span {class:"chip","ADVICE ≠ AN EXECUTED ORDER"}}p {"{s(&advice[\"reason\"])}"}
                    div {class:"actions",Action {label:"เปิดให้เสนอซื้อเพิ่ม",action:"enable_live"}Action {label:"สร้าง / ทดสอบสูตรใหม่",action:"research"}Action {label:"ใช้สูตรที่ทดสอบล่าสุด",action:"live_use_model"}}
                    p {class:"muted small","An explicitly selected research model uses RSI, MACD and other features. Optimization cannot change your reserve or trade limits. Targets follow each layer and estimated costs; there is no forced daily or weekly return."}
                    LiveField {label:"เงินที่ใช้ซื้อครั้งนี้ · USDC",value:amount}
                    div {class:"actions",Action {label:"เตรียมซื้อเมื่อผ่านโมเดล",action:"live_prepare",kind:"primary",payload:json!({"side":"buy","amount_usdc":amount()})}Action {label:"ซื้อเองด้วยเงินจริง (ข้ามโมเดล)",action:"live_prepare",payload:json!({"side":"buy","amount_usdc":amount(),"manual":true})}}
                    p {class:"muted small","ปุ่มซื้อเองใช้เงินจริงและข้ามเฉพาะสัญญาณโมเดล ยังตรวจวงเงินและความเสี่ยง กดเตรียมแล้วยังต้องเซ็นใน Phantom"}
                }
                }
            }else{
                section {class:"panel spaced live-order",div {class:"eyebrow","PENDING REAL ORDER"}h2 {"{s(&pending[\"side\"])} · {s(&pending[\"status\"])}"}
                    p {"Spend {spend_amount:.6} {spend_unit}"}
                    p {"Minimum receipt: {receive_amount:.6} {receive_unit}"}
                    p {"Estimated network fee: {n(&pending[\"network_fee\"])/1e9:.9} SOL"}
                    if pending["signature"].is_null(){div {class:"actions",button {class:"button primary",disabled:(ctx.busy)(),onclick:move |_|{spawn(async move{match invoke("open_live_order",json!({})).await{Ok(_)=>ctx.notice.set("Approval page opened. Check the wallet and amounts before signing in Phantom.".into()),Err(error)=>ctx.notice.set(error)}});},"ตรวจรายการและยืนยันใน Phantom ↗"}Action {label:"ยกเลิกรายการที่ยังไม่เซ็น",action:"live_cancel"}}}
                    else{p {class:"wallet-address","Signature: {s(&pending[\"signature\"])}"}p {"รอเครือข่ายยืนยัน อย่าส่งรายการเดิมซ้ำในหน้าต่างอื่น"}Action {label:"ตรวจผลคำสั่งเดิมอีกครั้ง",action:"live_reconcile"}}
                    if pending["message"].is_string(){p {class:"warning","{s(&pending[\"message\"])}"}}
                }
            }
            if account["reconciliation_error"].is_string(){p {class:"warning","{s(&account[\"reconciliation_error\"])}"}}
            section {class:"panel spaced",h2 {"SOL ที่ถือ แยกตามไม้"}p {class:"muted small","ยอดและต้นทุนของทั้งไม้รวมสำรองในไม้นี้ แต่ปุ่มขายใช้เฉพาะส่วนเทรด กำไรค้างยังไม่หักค่าขาย SOL สำหรับค่าธรรมเนียมที่อยู่นอกไม้ไม่ได้รวมในตารางนี้"}div {class:"layer-table",table {thead {tr {th {"ไม้ / เวลาบันทึก"}th {"จำนวน SOL"}th {"ต้นทุน · USDC / บาท"}th {"กำไรค้าง · USDC / บาท"}th {"เป้า / ทบทวน · USDC ต่อ SOL"}th {"เตรียมขายเอง"}}}tbody {for layer in items(&account["layers"]){tr {
                td {"#{n(&layer[\"id\"]) as u64}" div {class:"muted small","{local_time(n(&layer[\"opened_at\"]))}"}if layer["model"]=="imported-sol-from-original-capital" {div {class:"muted small","นำเข้า SOL เดิม"}}}td {"{n(&layer[\"quantity\"])/1e9:.9}" p {class:"muted small","สำรองในไม้ {n(&layer[\"reserved_quantity\"])/1e9:.9}"}}td {"{money(n(&layer[\"cost_usdc\"])/1e6)} / ฿{money(n(&layer[\"cost_satang\"])/100.0)}"}
                td {if advice["equity_usdc"].is_number(){"{money(n(&layer[\"quantity\"])/1e9*n(&data[\"price\"])-(n(&layer[\"cost_usdc\"])+n(&layer[\"entry_fee_usdc\"]))/1e6)} / ฿{money(n(&layer[\"quantity\"])/1e9*n(&data[\"price\"])*n(&data[\"thb_rate\"][\"micros\"])/1e6-(n(&layer[\"cost_satang\"])+n(&layer[\"entry_fee_satang\"]))/100.0)}"}else{"—"}}
                td {"{money(n(&layer[\"target_price\"])/1e6)} / {money(n(&layer[\"review_price\"])/1e6)}"}
                td {if n(&layer["quantity"])>n(&layer["reserved_quantity"])&&pending.is_null(){div {class:"actions compact",Action {label:"เตรียมขาย 50%",action:"live_prepare",payload:json!({"side":"sell","layer_id":layer["id"],"percent":50})}Action {label:"เตรียมขายทั้งหมด",action:"live_prepare",payload:json!({"side":"sell","layer_id":layer["id"],"percent":100})}}}}
            }}}}}
                p {class:"muted small","เป้า = ราคาที่เริ่มตรวจขายทำกำไร / ทบทวน = ราคาที่เตือนความเสี่ยง ไม่ใช่คำสั่งขายอัตโนมัติ ปุ่มขายเองอาจขายขาดทุนได้ ทุกครั้งยังต้องตรวจและเซ็นใน Phantom"}
                if items(&account["layers"]).iter().any(|l| l["model"]=="imported-sol-from-original-capital") {p {class:"muted small","ไม้ที่นำเข้า SOL เดิม: เป้าคำนวณจากต้นทุน USDC เดิม แต่จุดทบทวนอิงราคาเมื่อนำเข้า จึงอาจสูงกว่าเป้าได้ ส่วนที่กันไว้จ่ายค่าธรรมเนียมอยู่นอกไม้นี้"}}
            }
            section {class:"panel spaced",h2 {"ประวัติซื้อขายที่เครือข่ายยืนยันแล้ว"}p {class:"muted small","Only finalized transactions appear here. Realized P&L below excludes separately recorded network costs; the account totals above include them."}
                if items(&account["fills"]).is_empty(){p {"ยังไม่มีรายการซื้อขายที่ยืนยันแล้ว"}}
                for fill in items(&account["fills"]).into_iter().rev().take(30){div {class:"real-fill",strong {"{s(&fill[\"side\"])} · layer #{n(&fill[\"layer_id\"]) as u64}"}p {"Realized: {money(n(&fill[\"realized_usdc\"])/1e6)} USDC / ฿{money(n(&fill[\"realized_satang\"])/100.0)} · network fee {n(&fill[\"network_fee_lamports\"])/1e9:.9} SOL"}p {class:"wallet-address muted small","{s(&fill[\"signature\"])}"}}}
            }
            section {class:"panel spaced",h3 {"Risk watch"}p {"New buys pause at a layer review price, 10% loss against starting bot capital, or 50% loss against allocated historical baht cost. A review never guarantees an automatic sale."}
                p {class:"muted small",if data["email_configured"]==true{"Email provider configured. Delivery status appears in alerts."}else{"Email is not configured. Configure it privately in Settings if you want alerts while away."}}
                for alert in items(&data["alerts"]).into_iter().filter(|a|a["acknowledged"]!=true){p {class:"warning small","{s(&alert[\"message\"])}"}}Action {label:"Acknowledge review",action:"acknowledge"}
            }
        }
    }
}
#[component]
fn Settings() -> Element {
    let ctx = use_context::<Context>();
    let data = (ctx.state)();
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","WORKSTATION SETTINGS"}h1 {"Built to stay local."}p {"Crypto Bros 0.5.1 · manually approved trading"}}}
        div {class:"bottom-grid",
            updates_ui::Updates{}
            section {class:"panel",h2 {"Connections"}div {class:"watch-row",span {"Jupiter API key"}strong {if data["jupiter_key_configured"]==true{"CONFIGURED"}else{"KEYLESS QUOTES"}}}div {class:"watch-row",span {"Email notifications"}strong {if data["email_configured"]==true{"CONFIGURED"}else{"NOT CONFIGURED"}}}
                p {class:"muted small","Configure optional service credentials privately in the application data folder using the .env.example guide, then restart the engine. Secrets are never displayed here."}
                div {class:"divider"}div {class:"watch-row",span {"Wallet seed / key storage"}strong {"NOT SUPPORTED"}}div {class:"watch-row",span {"Real transaction signing"}strong {"YOU APPROVE IN PHANTOM"}}
            }
        }
        section {class:"panel spaced",h3 {"Background behavior"}p {"Closing the window keeps the paper engine running in the tray. Choose “Stop bot and quit” from the tray to stop it. A sleeping or offline computer cannot monitor the market or deliver alerts."}p {class:"muted small","Every service restart starts paused. Layers, history, research and feedback remain in the local database."}}
    }
}
