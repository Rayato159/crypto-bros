use crypto_bros::{
    api::{AppState, Engine},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify};

#[derive(Serialize, Deserialize)]
struct ConnectionInfo {
    port: u16,
    token: String,
    pid: u32,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Crypto Bros: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data_dir = args
        .iter()
        .position(|s| s == "--data-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .map_or_else(crypto_bros::data_directory, Ok)?;
    crypto_bros_core::privacy::protect_directory(&data_dir)?;
    let connection_path = data_dir.join("connection.json");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "Cannot create HTTP client")?;
    let operation = args
        .first()
        .filter(|s| !s.starts_with('-'))
        .map_or("serve", String::as_str);
    if operation != "serve" {
        let body = std::fs::read(&connection_path).map_err(|_| "Service is not running")?;
        let connection: ConnectionInfo =
            serde_json::from_slice(&body).map_err(|_| "Service connection is invalid")?;
        let base = format!("http://127.0.0.1:{}", connection.port);
        let response = if operation == "status" {
            client
                .get(format!("{base}/api/state"))
                .bearer_auth(&connection.token)
                .send()
                .await
        } else {
            let action = match operation {
                "stop" => "shutdown",
                "pause" => "pause",
                "start" => "start",
                _ => return Err("Commands: serve, status, start, pause, stop".into()),
            };
            client
                .post(format!("{base}/api/command"))
                .bearer_auth(&connection.token)
                .json(&json!({"action":action}))
                .send()
                .await
        }
        .map_err(|_| "Cannot reach service")?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|_| "Invalid service response")?;
        if operation == "status" {
            println!(
                "mode={} running={} source={} equity={} USDC",
                value["mode"], value["running"], value["source"], value["equity"]
            );
        } else {
            println!("{}", value);
        }
        return Ok(());
    }
    // Only the application, never a coding agent, loads user-managed runtime secrets.
    let service_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir.join("service.lock"))
        .map_err(|_| "Cannot open service lock")?;
    service_lock
        .try_lock()
        .map_err(|_| "A worker already owns this data directory")?;
    let _ = dotenvy::from_path(data_dir.join(".env"));
    let port = args
        .iter()
        .position(|s| s == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(48731);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|_| "Local port is already in use; service may already be running")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot determine local port")?
        .port();
    let store = Store::open(&data_dir.join("portfolio.sqlite"))?;
    let mut snapshot = store.load()?;
    snapshot.email_configured = [
        "CRYPTO_BROS_EMAIL_API_KEY",
        "CRYPTO_BROS_EMAIL_FROM",
        "CRYPTO_BROS_EMAIL_TO",
    ]
    .iter()
    .all(|key| std::env::var(key).is_ok_and(|s| !s.is_empty()));
    snapshot.jupiter_key_configured =
        std::env::var("CRYPTO_BROS_JUPITER_API_KEY").is_ok_and(|s| !s.is_empty());
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let connection = ConnectionInfo {
        port,
        token: token.clone(),
        pid: std::process::id(),
    };
    std::fs::write(
        &connection_path,
        serde_json::to_vec(&connection).map_err(|_| "Cannot serialize local session")?,
    )
    .map_err(|_| "Cannot write local session")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&connection_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot protect local session")?;
    }
    let state = AppState {
        engine: Arc::new(Mutex::new(Engine {
            snapshot,
            store,
            pair_ticket: None,
        })),
        token,
        client,
        shutdown: Arc::new(Notify::new()),
        data_dir,
        port,
    };
    let job = tokio::spawn(crypto_bros::api::background(state.clone()));
    let shutdown = state.shutdown.clone();
    println!(
        "Crypto Bros paper service is listening on 127.0.0.1:{port}. Live execution is disabled."
    );
    let result = axum::serve(listener, crypto_bros::api::router(state))
        .with_graceful_shutdown(async move {
            tokio::select! {_ = shutdown.notified()=>{},_ = tokio::signal::ctrl_c()=>{}}
        })
        .await
        .map_err(|_| "Local service stopped unexpectedly".to_string());
    job.abort();
    let _ = std::fs::remove_file(&connection_path);
    result
}
