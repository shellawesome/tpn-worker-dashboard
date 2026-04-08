use super::DbPool;
use tracing::info;

pub async fn init_database(pool: &DbPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dashboard_workers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            poll_enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    info!("dashboard_workers table initialized");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dashboard_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            worker_id TEXT NOT NULL REFERENCES dashboard_workers(id) ON DELETE CASCADE,
            polled_at TEXT NOT NULL,
            poll_latency_ms INTEGER NOT NULL,
            online INTEGER NOT NULL,
            error_message TEXT NOT NULL DEFAULT '',
            data TEXT NOT NULL DEFAULT '',
            version TEXT NOT NULL DEFAULT '',
            mode TEXT NOT NULL DEFAULT '',
            uptime_seconds INTEGER NOT NULL DEFAULT 0,
            registration_success INTEGER NOT NULL DEFAULT 0,
            wg_active_peers INTEGER NOT NULL DEFAULT 0,
            wg_max_peers INTEGER NOT NULL DEFAULT 0,
            proxy_available INTEGER NOT NULL DEFAULT 0,
            proxy_total INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_snapshots_worker_polled
         ON dashboard_snapshots (worker_id, polled_at)",
    )
    .execute(pool)
    .await?;
    info!("dashboard_snapshots table initialized");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dashboard_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    info!("dashboard_settings table initialized");

    Ok(())
}
