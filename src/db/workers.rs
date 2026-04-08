use super::DbPool;
use crate::error::AppError;
use sqlx::Row;

/// Insert a new worker.
pub async fn insert_worker(
    pool: &DbPool,
    id: &str,
    name: &str,
    url: &str,
    notes: &str,
    api_key: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO dashboard_workers (id, name, url, notes, api_key, poll_enabled, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 1, $6, $7)",
    )
    .bind(id)
    .bind(name)
    .bind(url)
    .bind(notes)
    .bind(api_key)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update an existing worker. Uses individual UPDATE statements per field.
pub async fn update_worker(
    pool: &DbPool,
    id: &str,
    name: Option<&str>,
    url: Option<&str>,
    notes: Option<&str>,
    api_key: Option<&str>,
    poll_enabled: Option<bool>,
) -> Result<bool, AppError> {
    // Check worker exists
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM dashboard_workers WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Ok(false);
    }

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(v) = name {
        sqlx::query("UPDATE dashboard_workers SET name = $1, updated_at = $2 WHERE id = $3")
            .bind(v).bind(&now).bind(id).execute(pool).await?;
    }
    if let Some(v) = url {
        sqlx::query("UPDATE dashboard_workers SET url = $1, updated_at = $2 WHERE id = $3")
            .bind(v).bind(&now).bind(id).execute(pool).await?;
    }
    if let Some(v) = notes {
        sqlx::query("UPDATE dashboard_workers SET notes = $1, updated_at = $2 WHERE id = $3")
            .bind(v).bind(&now).bind(id).execute(pool).await?;
    }
    if let Some(v) = api_key {
        sqlx::query("UPDATE dashboard_workers SET api_key = $1, updated_at = $2 WHERE id = $3")
            .bind(v).bind(&now).bind(id).execute(pool).await?;
    }
    if let Some(v) = poll_enabled {
        let val: i32 = if v { 1 } else { 0 };
        sqlx::query("UPDATE dashboard_workers SET poll_enabled = $1, updated_at = $2 WHERE id = $3")
            .bind(val).bind(&now).bind(id).execute(pool).await?;
    }

    Ok(true)
}

/// Delete a worker (snapshots cascade).
pub async fn delete_worker(pool: &DbPool, id: &str) -> Result<bool, AppError> {
    let result = sqlx::query("DELETE FROM dashboard_workers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List all workers with their latest snapshot.
pub async fn list_workers_with_latest(pool: &DbPool) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT w.id, w.name, w.url, w.notes, w.poll_enabled, w.created_at, w.updated_at,
                s.polled_at, s.poll_latency_ms, s.online, s.error_message,
                s.version, s.mode, s.uptime_seconds, s.registration_success,
                s.wg_active_peers, s.wg_max_peers, s.proxy_available, s.proxy_total
         FROM dashboard_workers w
         LEFT JOIN dashboard_snapshots s ON s.id = (
             SELECT id FROM dashboard_snapshots
             WHERE worker_id = w.id
             ORDER BY id DESC LIMIT 1
         )
         ORDER BY w.created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for row in &rows {
        let polled_at: Option<String> = row.try_get("polled_at").ok();
        let latest = if polled_at.is_some() {
            serde_json::json!({
                "online": row.get::<i32, _>("online") == 1,
                "polled_at": row.get::<String, _>("polled_at"),
                "poll_latency_ms": row.get::<i32, _>("poll_latency_ms"),
                "error_message": row.get::<String, _>("error_message"),
                "version": row.get::<String, _>("version"),
                "mode": row.get::<String, _>("mode"),
                "uptime_seconds": row.get::<i64, _>("uptime_seconds"),
                "registration_success": row.get::<i32, _>("registration_success") == 1,
                "wg_active_peers": row.get::<i32, _>("wg_active_peers"),
                "wg_max_peers": row.get::<i32, _>("wg_max_peers"),
                "proxy_available": row.get::<i64, _>("proxy_available"),
                "proxy_total": row.get::<i32, _>("proxy_total"),
            })
        } else {
            serde_json::Value::Null
        };

        result.push(serde_json::json!({
            "id": row.get::<String, _>("id"),
            "name": row.get::<String, _>("name"),
            "url": row.get::<String, _>("url"),
            "notes": row.get::<String, _>("notes"),
            "poll_enabled": row.get::<i32, _>("poll_enabled") == 1,
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "latest": latest,
        }));
    }

    Ok(result)
}

/// Get a single worker by ID.
pub async fn get_worker(pool: &DbPool, id: &str) -> Result<Option<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT id, name, url, notes, api_key, poll_enabled, created_at, updated_at
         FROM dashboard_workers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        serde_json::json!({
            "id": r.get::<String, _>("id"),
            "name": r.get::<String, _>("name"),
            "url": r.get::<String, _>("url"),
            "notes": r.get::<String, _>("notes"),
            "api_key": r.get::<String, _>("api_key"),
            "poll_enabled": r.get::<i32, _>("poll_enabled") == 1,
            "created_at": r.get::<String, _>("created_at"),
            "updated_at": r.get::<String, _>("updated_at"),
        })
    }))
}
