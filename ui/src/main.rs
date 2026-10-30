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
    ctx.notice
        .set(format!("Working · {}", action.replace('_', " ")));
    match invoke("command", json!({"action":action,"payload":payload})).await {
        Ok(_) => {
            ctx.notice
                .set("Action completed. Check the account and pending-order status below.".into());
            if let Ok(value) = invoke("snapshot", json!({})).await {
                ctx.state.set(value);
            }
        }
        Err(error) => ctx.notice.set(error),
    }
    ctx.busy.set(false);
}
#[component]
fn Action(
    label: String,
    action: String,
    #[props(default=json!({}))] payload: Value,
    #[props(default=String::from("secondary"))] kind: String,
) -> Element {
    let ctx = use_context::<Context>();
    rsx! {button {class:"button {kind}",disabled:(ctx.busy)(),onclick:move |_|{spawn(run_action(ctx,action.clone(),payload.clone()));},"{label}"}}
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
                Ok(value) => {
                    ctx.state.set(value);
                    if (ctx.notice)().starts_with("Connecting") {
                        ctx.notice
                            .set("Engine connected · real orders require Phantom approval".into());
                    }
                }
                Err(error) => {
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
        div {class:"shell",
            aside {class:"sidebar",div {class:"brand",div {class:"brand-mark","C↗"}div {class:"brand-name",div {"CRYPTO"}div {"BROS"}}}div {class:"workspace-label","PERSONAL TRADING LAB"}
                nav {for (index,name) in ["Overview","Live trading","Layers","Research","Decisions","Wallet","Settings"].into_iter().enumerate(){button {class:if tab()==name{"nav-item active"}else{"nav-item"},onclick:move |_|tab.set(name.into()),span {class:"nav-number","0{index+1}"}span {"{name}"}if tab()==name{span {class:"nav-arrow","↗"}}}}}
                div {class:"sidebar-bottom",div {class:"status-line",span {class:if running{"dot live"}else{"dot"}}if running{"PAPER ENGINE RUNNING"}else{"PAPER ENGINE PAUSED"}}p {"Your machine. Your data."}div {class:"version","v0.2.0 · MANUAL APPROVAL"}}
            }
            main {class:"main",header {class:"topbar",div {class:"breadcrumb","WORKSTATION",span {"/"},strong {"{tab()}"}}div {class:"topbar-right",span {class:"source-tag","{source_label}"}span {class:"paper-badge",if tab()=="Live trading"{"REAL MONEY · PHANTOM"}else if tab()=="Overview"||tab()=="Layers"{"PAPER TRADING"}else{"RESEARCH / ACCOUNT TOOLS"}}}}
                div {class:"content",
                    if source=="synthetic_demo"{div {class:"demo-banner","SYNTHETIC DATA · Prices, fills and results are simulated. They are not market performance."}}
                    if data["market_error"].is_string(){div {class:"warning","{s(&data[\"market_error\"])}"}}
                    match tab().as_str(){"Live trading"=>rsx!{LiveTrading{}},"Layers"=>rsx!{Layers{}},"Research"=>rsx!{Research{}},"Decisions"=>rsx!{Decisions{}},"Wallet"=>rsx!{Wallet{}},"Settings"=>rsx!{Settings{}},_=>rsx!{Overview{}}}
                    div {class:"notice",span {class:"notice-label",if busy(){"WORKING"}else{"ACTIVITY"}}span {"{notice()}"}}
                    footer {span {"LOCAL FIRST / SOLANA"}span {"Paper fills ≠ on-chain executions. Weekly target is a goal, never a guarantee."}}
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
    let chart = price_chart(&data["candles"]);
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","THE LONG GAME"}h1 {"A clear view."}p {"Observe the market. Test the edge. Protect the reserve."}}div {class:"actions",Action {label:"↻ Refresh market",action:"refresh"}Action {label:if running{"Pause engine"}else{"Start paper engine"},action:if running{"pause"}else{"start"},kind:"primary"}}}
        div {class:"metrics",Metric {label:"PAPER EQUITY",value:format!("{} USDC",money(equity)),detail:format!("{} total return · includes open positions",pct(n(&data["net_return"]))),accent:true}Metric {label:"PROTECTED RESERVE",value:money(if data.is_null(){400.0}else{reserve}),detail:"USDC · excluded from trading budget"}Metric {label:"AVAILABLE TO DEPLOY",value:money((cash-reserve-parked).max(0.0)),detail:format!("{open} / 4 layers active")}Metric {label:"DRAWDOWN",value:format!("{:.2}%",n(&data["drawdown"])*100.0),detail:"From paper equity high-water mark"}}
        div {class:"overview-grid",
            section {class:"panel chart-panel",div {class:"panel-heading",div {span {class:"pair-icon","◎"}strong {"SOL / USDC"}span {class:"muted","15m · closed candles"}}div {class:"chart-price",if n(&data["price"])>0.0{"{money(n(&data[\"price\"]))}"}else{"—"}}}
                div {class:"chart",dangerous_inner_html:chart}
                div {class:"indicator-row",div {span {"RSI · 14"}strong {"{n(&indicators[\"rsi\"]):.1}"}}div {span {"MACD HISTOGRAM"}strong {"{n(&indicators[\"histogram\"]):.4}"}}div {span {"ATR · 14"}strong {"{n(&indicators[\"atr\"]):.3}"}}}
                div {class:"panel-note","Chart source and swap quote are different markets. Paper fills include simulated fees and slippage."}
            }
            section {class:"panel allocation",div {class:"eyebrow","CAPITAL DISCIPLINE"}h2 {"Every dollar\nhas a job."}div {class:"allocation-bar",span {class:"reserve-segment"}span {class:"trading-segment"}}
                div {class:"allocation-line",span {class:"legend-square reserve"}span {"Reserve"}strong {"40%"}}div {class:"allocation-line",span {class:"legend-square trading"}span {"Trading budget"}strong {"60%"}}
                div {class:"divider"}div {class:"allocation-line",span {"Parked realized profit"}strong {"{money(parked)}"}}p {class:"muted small","Profit stays parked until you release it. The optimizer cannot access the reserve."}Action {label:"Release parked profit",action:"reinvest"}
            }
        }
        div {class:"bottom-grid",
            section {class:"panel",div {class:"panel-heading",h3 {"Next move"}span {class:"chip","MODEL + RISK CHECK"}}
                if data["active_model"].is_null(){div {class:"empty",h3 {"An edge needs evidence."}p {"Load candles, run research, then select a model for paper trading. Real accounts have separate controls under Live trading."}}}else{div {class:"model-summary",span {class:"big-label","WAIT / BUY / SELL"}p {"Active: {s(&data[\"active_model\"][\"version\"])}"}p {class:"muted","A forecast must clear uncertainty, costs and risk limits."}}}
                div {class:"actions",Action {label:"Run model research",action:"research"}Action {label:"Open paper layer",action:"buy"}}
            }
            section {class:"panel",div {class:"panel-heading",h3 {"System watch"}span {class:"chip","LOCAL ENGINE"}}
                div {class:"watch-row",span {"Real-money orders"}strong {"SEPARATE LIVE TAB"}}div {class:"watch-row",span {"Email risk alerts"}strong {if data["email_configured"]==true{"CONFIGURED"}else{"NOT CONFIGURED"}}}div {class:"watch-row",span {"Weekly target"}strong {"+5% / aspirational"}}
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
                    td {if n(&layer["quantity"])>0.0{div {class:"actions compact",Action {label:"Sell 50%",action:"sell",payload:json!({"id":layer["id"],"percent":50})}Action {label:"Close",action:"sell",payload:json!({"id":layer["id"],"percent":100})}}}else{span {class:"muted","CLOSED"}}}
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
        div {class:"page-heading",div {div {class:"eyebrow","REAL WALLET · YOUR SIGNATURE"}h1 {"Trade with a clear limit."}p {"USDC ↔ SOL · two layers · every order approved in Phantom"}}div {class:"actions",Action {label:"Refresh prices, FX & wallet",action:"live_refresh"}Action {label:"Pause new buys",action:"live_pause"}}}
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
            div {class:"metrics",Metric {label:"BOT EQUITY · USDC",value:metric("equity_usdc",1e6,""),detail:"Tracked USDC and open SOL at reference price",accent:true}Metric {label:"CURRENT VALUE · THB",value:metric("equity_satang",100.0,"฿"),detail:"USDC/THB reference, not a guaranteed cash-out rate"}Metric {label:"BOT P&L · USDC",value:metric("bot_pnl_usdc",1e6,""),detail:"Since onboarding, after recorded network costs"}Metric {label:"P&L VS ORIGINAL THB COST",value:metric("historical_pnl_satang",100.0,"฿"),detail:"Includes allocated prior price and FX changes"}}
            section {class:"panel spaced",div {class:"panel-heading",h2 {"Capital and valuation"}span {class:"chip",if data["live_enabled"]==true{"BUYS ENABLED · MANUAL SIGNING"}else{"NEW BUYS PAUSED"}}}
                div {class:"watch-row",span {"Original allocated cost"}strong {"฿{money(n(&account[\"initial_cost_satang\"])/100.0)}"}}
                div {class:"watch-row",span {"Bot P&L in baht since onboarding"}strong {"{metric(\"bot_pnl_satang\",100.0,\"฿\")}"}}
                div {class:"watch-row",span {"Tracked USDC cash / protected reserve"}strong {"{n(&account[\"cash\"])/1e6:.6} / {n(&account[\"config\"][\"reserve\"])/1e6:.6}"}}
                div {class:"watch-row",span {"Maximum active investment / per buy"}strong {"{money(n(&account[\"config\"][\"trading_budget\"])/1e6)} / {money(n(&account[\"config\"][\"max_buy\"])/1e6)} USDC"}}
                div {class:"watch-row",span {"Last checked on-chain USDC / native SOL"}strong {"{n(&data[\"real_balances\"][\"usdc\"])/1e6:.6} / {n(&data[\"real_balances\"][\"sol\"])/1e9:.6}"}}
                div {class:"watch-row",span {"USDC/THB reference"}strong {"฿{n(&data[\"thb_rate\"][\"micros\"])/1e6:.4}"}}
                p {class:"muted small","Coinbase reference fetched by the app; actual sale-to-baht proceeds may differ. Network costs are recorded separately from recoverable account rent. Refresh after transfers. External deposits do not automatically enlarge the bot's budget."}
                if account["funding_fees_included"]!=true{p {class:"warning small","Original funding fees were not confirmed as included. Historical baht P&L may omit those costs."}}
                div {class:"watch-row",span {"Parked realized profit"}strong {"{money(n(&account[\"parked_profit\"])/1e6)} USDC"}}Action {label:"Release profit within existing limits",action:"live_reinvest"}
            }
            if pending.is_null(){
                section {class:"panel spaced",div {class:"panel-heading",h2 {"Next move: {s(&advice[\"action\"])}"}span {class:"chip","ADVICE ≠ AN EXECUTED ORDER"}}p {"{s(&advice[\"reason\"])}"}
                    div {class:"actions",Action {label:"Enable manually approved buys",action:"enable_live"}Action {label:"Run model research",action:"research"}Action {label:"Use candidate for live advice",action:"live_use_model"}}
                    p {class:"muted small","An explicitly selected research model uses RSI, MACD and other features. Optimization cannot change your reserve or trade limits. A 5% weekly return is a target, not a promise."}
                    LiveField {label:"Buy amount · USDC",value:amount}
                    div {class:"actions",Action {label:"Prepare model buy",action:"live_prepare",kind:"primary",payload:json!({"side":"buy","amount_usdc":amount()})}Action {label:"Prepare manual test buy",action:"live_prepare",payload:json!({"side":"buy","amount_usdc":amount(),"manual":true})}}
                    p {class:"muted small","Manual test buy bypasses the model signal only. All spending, reserve and loss limits still apply. Preparing an order does not sign or submit it."}
                }
            }else{
                section {class:"panel spaced live-order",div {class:"eyebrow","PENDING REAL ORDER"}h2 {"{s(&pending[\"side\"])} · {s(&pending[\"status\"])}"}
                    p {"Spend {spend_amount:.6} {spend_unit}"}
                    p {"Minimum receipt: {receive_amount:.6} {receive_unit}"}
                    p {"Estimated network fee: {n(&pending[\"network_fee\"])/1e9:.9} SOL"}
                    if pending["signature"].is_null(){div {class:"actions",button {class:"button primary",disabled:(ctx.busy)(),onclick:move |_|{spawn(async move{match invoke("open_live_order",json!({})).await{Ok(_)=>ctx.notice.set("Approval page opened. Check the wallet and amounts before signing in Phantom.".into()),Err(error)=>ctx.notice.set(error)}});},"Review and approve in Phantom ↗"}Action {label:"Cancel unsigned order",action:"live_cancel"}}}
                    else{p {class:"wallet-address","Signature: {s(&pending[\"signature\"])}"}p {"Waiting for finalized chain confirmation. Do not repeat the trade in another window."}Action {label:"Reconcile transaction now",action:"live_reconcile"}}
                    if pending["message"].is_string(){p {class:"warning","{s(&pending[\"message\"])}"}}
                }
            }
            if account["reconciliation_error"].is_string(){p {class:"warning","{s(&account[\"reconciliation_error\"])}"}}
            section {class:"panel spaced",h2 {"Real SOL layers"}p {class:"muted small","Unrealized P&L includes allocated entry network costs; future exit costs are not yet known."}table {thead {tr {th {"LAYER / OPENED"}th {"SOL"}th {"COST · USDC / THB"}th {"UNREALIZED · USDC / THB"}th {"TARGET / REVIEW · USDC"}th {"PREPARE EXIT"}}}tbody {for layer in items(&account["layers"]){tr {
                td {"#{n(&layer[\"id\"]) as u64}" div {class:"muted small","{local_time(n(&layer[\"opened_at\"]))}"}}td {"{n(&layer[\"quantity\"])/1e9:.9}"}td {"{money(n(&layer[\"cost_usdc\"])/1e6)} / ฿{money(n(&layer[\"cost_satang\"])/100.0)}"}
                td {if advice["equity_usdc"].is_number(){"{money(n(&layer[\"quantity\"])/1e9*n(&data[\"price\"])-(n(&layer[\"cost_usdc\"])+n(&layer[\"entry_fee_usdc\"]))/1e6)} / ฿{money(n(&layer[\"quantity\"])/1e9*n(&data[\"price\"])*n(&data[\"thb_rate\"][\"micros\"])/1e6-(n(&layer[\"cost_satang\"])+n(&layer[\"entry_fee_satang\"]))/100.0)}"}else{"—"}}
                td {"{money(n(&layer[\"target_price\"])/1e6)} / {money(n(&layer[\"review_price\"])/1e6)}"}
                td {if n(&layer["quantity"])>0.0&&pending.is_null(){div {class:"actions compact",Action {label:"Sell 50%",action:"live_prepare",payload:json!({"side":"sell","layer_id":layer["id"],"percent":50})}Action {label:"Close layer",action:"live_prepare",payload:json!({"side":"sell","layer_id":layer["id"],"percent":100})}}}}
            }}}}}
            section {class:"panel spaced",h2 {"Confirmed chain journal"}p {class:"muted small","Only finalized transactions appear here. Realized P&L below excludes separately recorded network costs; the account totals above include them."}
                if items(&account["fills"]).is_empty(){p {"No finalized transactions recorded yet."}}
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
    let mut ctx = use_context::<Context>();
    let data = (ctx.state)();
    let mut update = use_signal(|| Value::Null);
    rsx! {
        div {class:"page-heading",div {div {class:"eyebrow","WORKSTATION SETTINGS"}h1 {"Built to stay local."}p {"Crypto Bros 0.2.0 · manually approved trading"}}}
        div {class:"bottom-grid",
            section {class:"panel",h2 {"Application updates"}p {"Signed releases update the app while preserving your portfolio. The engine pauses and the database is backed up first."}
                button {class:"button primary",onclick:move |_|{spawn(async move {match invoke("check_update",json!({})).await{Ok(v)=>{ctx.notice.set(s(&v["message"]).to_string());update.set(v);},Err(e)=>ctx.notice.set(e)}});},"Check for updates"}
                if update()["available"]==true{p {"New version: {s(&update()[\"version\"])}"}button {class:"button secondary",onclick:move |_|{spawn(async move {match invoke("install_update",json!({})).await{Ok(_)=>ctx.notice.set("Installing verified update…".into()),Err(e)=>ctx.notice.set(e)}});},"Pause and install update"}}
                if !update().is_null() && update()["configured"]==false{p {class:"muted small","No published update channel for this local preview. A release owner must configure its public verification key and release URL."}}
                if update()["configured"]==true && update()["available"]==false{p {"You are up to date."}}
            }
            section {class:"panel",h2 {"Connections"}div {class:"watch-row",span {"Jupiter API key"}strong {if data["jupiter_key_configured"]==true{"CONFIGURED"}else{"KEYLESS QUOTES"}}}div {class:"watch-row",span {"Email notifications"}strong {if data["email_configured"]==true{"CONFIGURED"}else{"NOT CONFIGURED"}}}
                p {class:"muted small","Configure optional service credentials privately in the application data folder using the .env.example guide, then restart the engine. Secrets are never displayed here."}
                div {class:"divider"}div {class:"watch-row",span {"Wallet seed / key storage"}strong {"NOT SUPPORTED"}}div {class:"watch-row",span {"Real transaction signing"}strong {"YOU APPROVE IN PHANTOM"}}
            }
        }
        section {class:"panel spaced",h3 {"Background behavior"}p {"Closing the window keeps the paper engine running in the tray. Choose “Stop bot and quit” from the tray to stop it. A sleeping or offline computer cannot monitor the market or deliver alerts."}p {class:"muted small","Every service restart starts paused. Layers, history, research and feedback remain in the local database."}}
    }
}
fn price_chart(value: &Value) -> String {
    let all = items(value);
    let candles: Vec<_> = all.iter().rev().take(80).rev().collect();
    if candles.is_empty() {
        return "<div class='chart-empty'><span class='chart-empty-icon'>↗</span><h3>The market, in context.</h3><p>Refresh SOL/USDC candles, or load the synthetic demo below.</p></div>".into();
    }
    let low = candles
        .iter()
        .map(|c| n(&c["low"]))
        .fold(f64::INFINITY, f64::min);
    let high = candles
        .iter()
        .map(|c| n(&c["high"]))
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (high - low).max(0.01);
    let y = |p: f64| 270.0 - (p - low) / range * 230.0;
    let mut svg =
        String::from("<svg viewBox='0 0 960 310' role='img' aria-label='SOL USDC candle chart'>");
    for i in 0..5 {
        let py = 40.0 + i as f64 * 57.5;
        let price = high - i as f64 * range / 4.0;
        svg.push_str(&format!("<line x1='10' y1='{py}' x2='875' y2='{py}' stroke='#2a2f25' stroke-dasharray='3 5'/><text x='892' y='{}' fill='#89937e' font-family='monospace' font-size='12'>{price:.2}</text>",py+4.0));
    }
    let step = 850.0 / candles.len() as f64;
    for (i, c) in candles.iter().enumerate() {
        let x = 14.0 + i as f64 * step;
        let open = n(&c["open"]);
        let close = n(&c["close"]);
        let color = if close >= open { "#b8e787" } else { "#c77c66" };
        let top = y(open.max(close));
        let height = (y(open.min(close)) - top).max(1.5);
        svg.push_str(&format!("<line x1='{x}' y1='{}' x2='{x}' y2='{}' stroke='{color}'/><rect x='{}' y='{top}' width='{}' height='{height}' fill='{color}' rx='1'/>",y(n(&c["high"])),y(n(&c["low"])),x-step*0.26,step*0.52));
    }
    svg.push_str("<text x='14' y='302' fill='#6f7965' font-family='monospace' font-size='11'>COMPLETED 15-MINUTE CANDLES</text></svg>");
    svg
}
