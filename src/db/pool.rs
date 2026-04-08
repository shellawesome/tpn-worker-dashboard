use super::DbPool;
use crate::config::DashboardConfig;
use sqlx::sqlite::SqlitePoolOptions;

pub async fn create_pool(config: &DashboardConfig) -> Result<DbPool, sqlx::Error> {
    let url = format!("sqlite:{}?mode=rwc", config.sqlite_path);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
    sqlx::query("PRAGMA busy_timeout=5000").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    Ok(pool)
}
