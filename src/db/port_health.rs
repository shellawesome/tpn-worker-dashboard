use super::DbPool;
use crate::error::AppError;
use sqlx::Row;
use std::collections::HashMap;

pub struct PortHealthRow {
    pub worker_id: String,
    pub checked_at: String,
    pub host: String,
    pub public_port: i32,
    pub public_ok: bool,
    pub socks5_port: i32,
    pub socks5_ok: bool,
    pub http_port: i32,
    pub http_ok: bool,
    pub wg_port: i32,
    pub wg_ok: bool,
}

pub async fn upsert(pool: &DbPool, r: &PortHealthRow) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO dashboard_port_health
         (worker_id, checked_at, host,
          public_port, public_ok, socks5_port, socks5_ok,
          http_port, http_ok, wg_port, wg_ok)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT(worker_id) DO UPDATE SET
          checked_at=excluded.checked_at,
          host=excluded.host,
          public_port=excluded.public_port,
          public_ok=excluded.public_ok,
          socks5_port=excluded.socks5_port,
          socks5_ok=excluded.socks5_ok,
          http_port=excluded.http_port,
          http_ok=excluded.http_ok,
          wg_port=excluded.wg_port,
          wg_ok=excluded.wg_ok",
    )
    .bind(&r.worker_id)
    .bind(&r.checked_at)
    .bind(&r.host)
    .bind(r.public_port)
    .bind(if r.public_ok { 1 } else { 0 })
    .bind(r.socks5_port)
    .bind(if r.socks5_ok { 1 } else { 0 })
    .bind(r.http_port)
    .bind(if r.http_ok { 1 } else { 0 })
    .bind(r.wg_port)
    .bind(if r.wg_ok { 1 } else { 0 })
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_all(pool: &DbPool) -> Result<HashMap<String, serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT worker_id, checked_at, host,
                public_port, public_ok, socks5_port, socks5_ok,
                http_port, http_ok, wg_port, wg_ok
         FROM dashboard_port_health",
    )
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for row in rows {
        let worker_id: String = row.get("worker_id");
        map.insert(
            worker_id.clone(),
            serde_json::json!({
                "checked_at": row.get::<String, _>("checked_at"),
                "host": row.get::<String, _>("host"),
                "public_port": row.get::<i32, _>("public_port"),
                "public_ok": row.get::<i32, _>("public_ok") == 1,
                "socks5_port": row.get::<i32, _>("socks5_port"),
                "socks5_ok": row.get::<i32, _>("socks5_ok") == 1,
                "http_port": row.get::<i32, _>("http_port"),
                "http_ok": row.get::<i32, _>("http_ok") == 1,
                "wg_port": row.get::<i32, _>("wg_port"),
                "wg_ok": row.get::<i32, _>("wg_ok") == 1,
            }),
        );
    }
    Ok(map)
}
