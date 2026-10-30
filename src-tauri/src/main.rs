#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize, Deserialize)]
struct ConnectionInfo {
    port: u16,
    token: String,
    pid: u32,
}
struct Backend {
    client: reqwest::Client,
    data_dir: PathBuf,
    startup_error: Option<String>,
    focus_live: AtomicBool,
    notification_status: Mutex<String>,
}
impl Backend {
    fn connection(&self) -> Result<ConnectionInfo, String> {
        let bytes = std::fs::read(self.data_dir.join("connection.json"))
            .map_err(|_| "Background service is unavailable")?;
        serde_json::from_slice(&bytes)
            .map_err(|_| "Background service connection is invalid".into())
    }
    async fn state(&self) -> Result<Value, String> {
        if let Some(error) = &self.startup_error {
            return Err(error.clone());
        }
        let c = self.connection()?;
        let response = self
            .client
            .get(format!("http://127.0.0.1:{}/api/state", c.port))
            .timeout(Duration::from_secs(2))
            .bearer_auth(c.token)
            .send()
            .await
            .map_err(|_| "Cannot reach background service")?;
        response
            .error_for_status()
            .map_err(|_| "Background service rejected request")?
            .json()
            .await
            .map_err(|_| "Invalid service response".into())
    }
    async fn command(&self, action: &str, payload: Value) -> Result<Value, String> {
        if let Some(error) = &self.startup_error {
            return Err(error.clone());
        }
        let c = self.connection()?;
        let response = self
            .client
            .post(format!("http://127.0.0.1:{}/api/command", c.port))
            .bearer_auth(c.token)
            .json(&json!({"action":action,"payload":payload}))
            .send()
            .await
            .map_err(|_| "Cannot reach background service")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Invalid service response")?;
        if !status.is_success() {
            return Err(body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Command failed")
                .into());
        }
        Ok(body)
    }
}
#[tauri::command]
async fn snapshot(backend: tauri::State<'_, Backend>) -> Result<Value, String> {
    let mut value = backend.state().await?;
    value["focus_live"] = json!(backend.focus_live.swap(false, Ordering::Relaxed));
    value["notification_status"] = json!(
        backend
            .notification_status
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default()
    );
    Ok(value)
}
#[tauri::command]
async fn command(
    backend: tauri::State<'_, Backend>,
    action: String,
    payload: Value,
) -> Result<Value, String> {
    if ![
        "refresh",
        "research",
        "quote",
        "balance",
        "demo",
        "demo_step",
        "archive_demo",
        "activate_paper",
        "start",
        "pause",
        "buy",
        "sell",
        "reinvest",
        "feedback",
        "acknowledge",
        "wallet_address",
        "live_refresh",
        "live_setup",
        "enable_live",
        "live_pause",
        "live_use_model",
        "live_reinvest",
        "live_cancel",
        "live_prepare",
        "live_reconcile",
        "live_watch_start",
        "live_watch_stop",
        "live_watch_snooze",
    ]
    .contains(&action.as_str())
    {
        return Err("Command is not available from the dashboard".into());
    }
    backend.command(&action, payload).await
}
#[tauri::command]
async fn review_watch_order(
    app: tauri::AppHandle,
    backend: tauri::State<'_, Backend>,
    key: String,
) -> Result<(), String> {
    backend
        .command("live_watch_prepare", json!({"key":key}))
        .await?;
    open_live_order(app, backend).await
}
#[tauri::command]
fn test_notification(app: tauri::AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Crypto Bros · ทดสอบแจ้งเตือน")
        .body("นี่คือการทดสอบเท่านั้น ไม่มีคำสั่งซื้อขาย กดที่ข้อความนี้เพื่อเปิดหน้าเฝ้าตลาด")
        .show()
        .map_err(|_| "ส่งคำแจ้งเตือนไม่ได้ ตรวจการตั้งค่าแจ้งเตือนของ Windows".to_string())
}
fn focus_live(app: &tauri::AppHandle) {
    app.state::<Backend>()
        .focus_live
        .store(true, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
fn start_notifications(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let backend = app.state::<Backend>();
            let Ok(response) = backend.command("live_watch_claim", json!({})).await else {
                continue;
            };
            let signal = &response["signal"];
            let Some(title) = signal["title"].as_str() else {
                continue;
            };
            let body = format!(
                "{} · กดเปิดแอปเพื่อตรวจราคาปัจจุบันและยืนยันใน Phantom",
                signal["reason"].as_str().unwrap_or("มีรายการให้ตรวจสอบ")
            );
            let result = app
                .notification()
                .builder()
                .title(format!("Crypto Bros · {title}"))
                .body(body)
                .show();
            if let Ok(mut status) = backend.notification_status.lock() {
                *status = if result.is_ok() {
                    "ส่งคำแจ้งเตือนไปยัง Windows แล้ว; โหมดห้ามรบกวนอาจซ่อนข้อความ"
                } else {
                    "ส่งคำแจ้งเตือนไม่สำเร็จ ดูรายการในแอปและตรวจการตั้งค่า Windows"
                }
                .into();
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ =
                    window.request_user_attention(Some(tauri::UserAttentionType::Informational));
            }
        }
    });
}
#[tauri::command]
async fn connect_wallet(
    app: tauri::AppHandle,
    backend: tauri::State<'_, Backend>,
) -> Result<(), String> {
    let response = backend.command("wallet_link", json!({})).await?;
    let url = response["url"]
        .as_str()
        .ok_or("Invalid wallet connection link")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Could not open your default browser".into())
}
#[tauri::command]
async fn open_live_order(
    app: tauri::AppHandle,
    backend: tauri::State<'_, Backend>,
) -> Result<(), String> {
    let response = backend.command("live_link", json!({})).await?;
    let url = response["url"].as_str().ok_or("Invalid approval link")?;
    // The signing page runs in the browser extension context, never in the desktop webview.
    app.opener()
        .open_url(url, Some("chrome"))
        .or_else(|_| app.opener().open_url(url, None::<&str>))
        .map_err(|_| "Could not open the approval page in a browser".into())
}
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Value, String> {
    let configured = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("endpoints"))
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty());
    if !configured {
        return Ok(
            json!({"configured":false,"message":"This local preview is not connected to a published update channel. Signed releases will enable in-app updates."}),
        );
    }
    let update = app
        .updater()
        .map_err(|_| "Updater configuration is invalid")?
        .check()
        .await
        .map_err(|_| "Could not check the signed release channel")?;
    Ok(match update {
        Some(u) => json!({"configured":true,"available":true,"version":u.version,"notes":u.body}),
        None => json!({"configured":true,"available":false}),
    })
}
#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    backend: tauri::State<'_, Backend>,
) -> Result<(), String> {
    // Resolve release availability before interrupting the running worker.
    let update = app
        .updater()
        .map_err(|_| "Updater not configured")?
        .check()
        .await
        .map_err(|_| "Could not check updates")?
        .ok_or("No update is available")?;
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|_| "Update download or signature verification failed")?;
    backend.command("prepare_update", json!({})).await?;
    backend.command("shutdown", json!({})).await?;
    for _ in 0..50 {
        if !backend.data_dir.join("connection.json").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if backend.data_dir.join("connection.json").exists() {
        return Err("Worker has not stopped; installation was cancelled".into());
    }
    update
        .install(bytes)
        .map_err(|_| "Installer failed; your portfolio backup was preserved".into())
}
fn data_dir() -> Result<PathBuf, String> {
    let args: Vec<_> = std::env::args().collect();
    if let Some(path) = args
        .iter()
        .position(|a| a == "--data-dir")
        .and_then(|i| args.get(i + 1))
    {
        return Ok(PathBuf::from(path));
    }
    directories::ProjectDirs::from("dev", "CryptoBros", "CryptoBros")
        .map(|d| d.data_local_dir().to_path_buf())
        .ok_or("Cannot locate local data directory".into())
}
fn ensure_worker(
    app: &tauri::AppHandle,
    dir: &std::path::Path,
    backend: &Backend,
) -> Result<(), String> {
    if let Ok(value) = tauri::async_runtime::block_on(backend.state()) {
        if value["app_version"] != env!("CARGO_PKG_VERSION") {
            tauri::async_runtime::block_on(backend.command("prepare_update",json!({})))
                .map_err(|e|format!("The older worker could not prepare an update: {e}. Reopen the older app to resolve pending transactions."))?;
            tauri::async_runtime::block_on(backend.command("shutdown", json!({})))?;
            for _ in 0..50 {
                if !dir.join("connection.json").exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if dir.join("connection.json").exists() {
                return Err(
                    "The previous worker has not stopped; restart the app after it exits".into(),
                );
            }
        } else {
            return Ok(());
        }
    }
    let resource = app
        .path()
        .resource_dir()
        .map_err(|_| "Cannot locate bundled worker")?
        .join("crypto-bros.exe");
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/crypto-bros.exe");
    let worker = if resource.exists() {
        resource
    } else {
        development
    };
    if !worker.exists() {
        return Err(
            "Worker is missing. Reinstall the MSI, or run npm run build when building from source"
                .into(),
        );
    }
    let mut process = std::process::Command::new(worker);
    process
        .arg("serve")
        .arg("--data-dir")
        .arg(dir)
        .arg("--port")
        .arg("0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    process
        .spawn()
        .map_err(|_| "Could not start background worker")?;
    for _ in 0..50 {
        if tauri::async_runtime::block_on(backend.state()).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(
        "Worker did not become ready. Reopen the app and keep the existing account data intact."
            .into(),
    )
}
fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = data_dir()?;
            crypto_bros_core::privacy::protect_directory(&dir)?;
            let mut backend = Backend {
                client: reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(60))
                    .build()?,
                data_dir: dir.clone(),
                startup_error: None,
                focus_live: AtomicBool::new(false),
                notification_status: Mutex::new("กดทดสอบแจ้งเตือนก่อนซ่อนแอปไว้ในถาดระบบ".into()),
            };
            if let Err(error) = ensure_worker(app.handle(), &dir, &backend) {
                backend.startup_error = Some(error);
            }
            app.manage(backend);
            let notification_app = app.handle().clone();
            app.notification()
                .on_action(move |_| focus_live(&notification_app))?;
            start_notifications(app.handle().clone());
            let open = MenuItem::with_id(app, "open", "Open Crypto Bros", true, None::<&str>)?;
            let pause = MenuItem::with_id(
                app,
                "pause",
                "Pause trading and new buys",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Stop bot and quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Crypto Bros · เฝ้าสัญญาณ / ยืนยันทุกไม้ใน Phantom")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "pause" | "quit" => {
                        let app = app.clone();
                        let quit = event.id.as_ref() == "quit";
                        tauri::async_runtime::spawn(async move {
                            let backend = app.state::<Backend>();
                            let result = backend
                                .command(if quit { "shutdown" } else { "pause" }, json!({}))
                                .await;
                            if quit && result.is_ok() {
                                app.exit(0);
                            }
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            command,
            connect_wallet,
            open_live_order,
            review_watch_order,
            test_notification,
            check_update,
            install_update
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(error) => {
            eprintln!("Crypto Bros startup failed: {error}");
            std::process::exit(1);
        }
    }
}
