use axum::routing::{delete, get, post, put};
use axum::Router;
use clap::Parser;
use db::DbPool;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

mod api;
mod config;
mod db;
mod error;
mod models;
mod poller;

use config::DashboardConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: Arc<DashboardConfig>,
    pub http_client: reqwest::Client,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

// ── Config directory bootstrap ──

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home)
        .join(".config")
        .join("tpn-worker-dashboard")
}

fn ensure_config_dir(dir: &PathBuf) {
    if !dir.exists() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| {
            eprintln!("Failed to create config directory {:?}: {}", dir, e);
            std::process::exit(1);
        });
    }
}

fn default_env_content() -> String {
    r#"# TPN Worker Dashboard 配置
# DASHBOARD_PORT=8080
# DASHBOARD_POLL_INTERVAL=30
# DASHBOARD_POLL_TIMEOUT=10
# LOG_LEVEL=info
"#
    .to_string()
}

fn ensure_env_file(dir: &PathBuf) -> PathBuf {
    let env_path = dir.join(".env");
    if !env_path.exists() {
        std::fs::write(&env_path, default_env_content()).unwrap_or_else(|e| {
            eprintln!("Failed to write default .env at {:?}: {}", env_path, e);
            std::process::exit(1);
        });
        eprintln!("Generated default config: {}", env_path.display());
    }
    env_path
}

fn load_env(dir: &PathBuf) {
    let env_path = dir.join(".env");
    let _ = dotenvy::from_path(&env_path);

    if std::env::var("DASHBOARD_SQLITE_PATH").is_err() {
        std::env::set_var("DASHBOARD_SQLITE_PATH", dir.join("dashboard.db"));
    }
}

// ── Main ──

#[tokio::main]
async fn main() {
    // 0. Bootstrap config directory
    let dir = config_dir();
    ensure_config_dir(&dir);
    ensure_env_file(&dir);
    load_env(&dir);

    // 1. Parse config
    let config = DashboardConfig::parse();

    // 2. Initialize tracing
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    info!("Starting TPN Worker Dashboard v{}", env!("CARGO_PKG_VERSION"));
    info!("Config directory: {}", dir.display());
    info!("Database path: {}", config.sqlite_path);

    // 3. Connect to SQLite
    let pool = match db::pool::create_pool(&config).await {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to connect to database: {}", e);
            std::process::exit(1);
        }
    };

    // 4. Initialize tables
    if let Err(e) = db::init::init_database(&pool).await {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }

    // 5. Build HTTP client for polling
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.poll_timeout_seconds))
        .build()
        .expect("Failed to build HTTP client");

    // 6. Build shared state
    let config = Arc::new(config);
    let state = AppState {
        db: pool.clone(),
        config: config.clone(),
        http_client: http_client.clone(),
        start_time: chrono::Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // 7. Build router
    let app = Router::new()
        .route("/", get(api::dashboard::redirect_to_dashboard))
        .route("/dashboard", get(api::dashboard::dashboard_page))
        .route("/api/workers", get(api::workers::list_workers))
        .route("/api/workers", post(api::workers::create_worker))
        .route("/api/workers/{id}", get(api::workers::list_workers)) // detail via frontend
        .route("/api/workers/{id}", put(api::workers::update_worker))
        .route("/api/workers/{id}", delete(api::workers::delete_worker))
        .route(
            "/api/workers/{id}/history",
            get(api::workers::worker_history),
        )
        .route(
            "/api/workers/{id}/poll",
            post(api::workers::poll_worker_now),
        )
        .route(
            "/api/workers/{id}/logs",
            get(api::workers::worker_logs),
        )
        .route(
            "/api/workers/{id}/ports",
            post(api::workers::update_worker_ports),
        )
        .route("/api/batch/version", post(api::batch::batch_check_version))
        .route("/api/batch/upgrade", post(api::batch::batch_upgrade))
        .route("/api/batch/config", post(api::batch::batch_config))
        .route("/api/batch/password", post(api::batch::batch_set_password))
        .route("/api/batch/auth", post(api::batch::batch_set_auth))
        .route("/api/batch/restart", post(api::batch::batch_restart))
        .route("/api/batch/stop", post(api::batch::batch_stop))
        .route("/api/batch/start", post(api::batch::batch_start))
        .with_state(state);

    // 8. Spawn background poller
    let poll_interval = config.poll_interval_seconds;
    tokio::spawn(async move {
        poller::run_poller(pool, http_client, poll_interval).await;
    });

    // 9. Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("HTTP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind HTTP listener");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("HTTP server error");

    info!("TPN Worker Dashboard shutting down gracefully");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C"),
        _ = terminate => info!("Received SIGTERM"),
    }
}
