use super::DbPool;
use crate::error::AppError;
use sqlx::Row;

/// Insert a snapshot record.
pub async fn insert_snapshot(
    pool: &DbPool,
    worker_id: &str,
    polled_at: &str,
    poll_latency_ms: i64,
    online: bool,
    error_message: &str,
    data: &str,
    version: &str,
    mode: &str,
    uptime_seconds: i64,
    registration_success: bool,
    wg_active_peers: i32,
    wg_max_peers: i32,
    proxy_available: i64,
    proxy_total: i32,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO dashboard_snapshots
         (worker_id, polled_at, poll_latency_ms, online, error_message, data,
          version, mode, uptime_seconds, registration_success,
          wg_active_peers, wg_max_peers, proxy_available, proxy_total)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(worker_id)
    .bind(polled_at)
    .bind(poll_latency_ms)
    .bind(if online { 1 } else { 0 })
    .bind(error_message)
    .bind(data)
    .bind(version)
    .bind(mode)
    .bind(uptime_seconds)
    .bind(if registration_success { 1 } else { 0 })
    .bind(wg_active_peers)
    .bind(wg_max_peers)
    .bind(proxy_available)
    .bind(proxy_total)
    .execute(pool)
    .await?;

    // Retention: keep last 1000 per worker
    sqlx::query(
        "DELETE FROM dashboard_snapshots
         WHERE worker_id = $1 AND id NOT IN (
             SELECT id FROM dashboard_snapshots
             WHERE worker_id = $2
             ORDER BY id DESC LIMIT 1000
         )",
    )
    .bind(worker_id)
    .bind(worker_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch recent snapshots for a worker.
pub async fn worker_history(
    pool: &DbPool,
    worker_id: &str,
    limit: i32,
    offset: i32,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, polled_at, poll_latency_ms, online, error_message, data,
                version, mode, uptime_seconds, registration_success,
                wg_active_peers, wg_max_peers, proxy_available, proxy_total
         FROM dashboard_snapshots
         WHERE worker_id = $1
         ORDER BY id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(worker_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "polled_at": r.get::<String, _>("polled_at"),
                "poll_latency_ms": r.get::<i64, _>("poll_latency_ms"),
                "online": r.get::<i32, _>("online") == 1,
                "error_message": r.get::<String, _>("error_message"),
                "data": r.get::<String, _>("data"),
                "version": r.get::<String, _>("version"),
                "mode": r.get::<String, _>("mode"),
                "uptime_seconds": r.get::<i64, _>("uptime_seconds"),
                "registration_success": r.get::<i32, _>("registration_success") == 1,
                "wg_active_peers": r.get::<i32, _>("wg_active_peers"),
                "wg_max_peers": r.get::<i32, _>("wg_max_peers"),
                "proxy_available": r.get::<i64, _>("proxy_available"),
                "proxy_total": r.get::<i32, _>("proxy_total"),
            })
        })
        .collect())
}
