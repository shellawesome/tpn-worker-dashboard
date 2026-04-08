use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::{snapshots, workers as db_workers};
use crate::error::AppError;
use crate::poller;
use crate::AppState;

/// GET /api/workers — List all workers with latest snapshot.
pub async fn list_workers(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let workers = db_workers::list_workers_with_latest(&state.db).await?;
    Ok(Json(json!(workers)))
}

/// POST /api/workers — Create a new worker.
#[derive(Deserialize)]
pub struct CreateWorkerRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub api_key: String,
}

pub async fn create_worker(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkerRequest>,
) -> Result<Json<Value>, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if body.url.trim().is_empty() {
        return Err(AppError::BadRequest("url is required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    db_workers::insert_worker(&state.db, &id, body.name.trim(), body.url.trim(), &body.notes, &body.api_key).await?;

    // Trigger immediate poll
    let pool = state.db.clone();
    let client = state.http_client.clone();
    tokio::spawn(async move {
        poller::poll_all_workers(&pool, &client).await;
    });

    let worker = db_workers::get_worker(&state.db, &id).await?;
    Ok(Json(json!(worker)))
}

/// PUT /api/workers/:id — Update a worker.
#[derive(Deserialize)]
pub struct UpdateWorkerRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub api_key: Option<String>,
    pub poll_enabled: Option<bool>,
}

pub async fn update_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkerRequest>,
) -> Result<Json<Value>, AppError> {
    let found = db_workers::update_worker(
        &state.db,
        &id,
        body.name.as_deref(),
        body.url.as_deref(),
        body.notes.as_deref(),
        body.api_key.as_deref(),
        body.poll_enabled,
    )
    .await?;

    if !found {
        return Err(AppError::NotFound(format!("Worker {} not found", id)));
    }

    let worker = db_workers::get_worker(&state.db, &id).await?;
    Ok(Json(json!(worker)))
}

/// DELETE /api/workers/:id — Delete a worker.
pub async fn delete_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let found = db_workers::delete_worker(&state.db, &id).await?;
    if !found {
        return Err(AppError::NotFound(format!("Worker {} not found", id)));
    }
    Ok(Json(json!({ "deleted": true })))
}

/// GET /api/workers/:id/history — Snapshot history.
#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}
fn default_limit() -> i32 { 100 }

pub async fn worker_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Value>, AppError> {
    let history = snapshots::worker_history(&state.db, &id, q.limit, q.offset).await?;
    Ok(Json(json!(history)))
}

/// POST /api/workers/:id/poll — Manual poll trigger.
pub async fn poll_worker_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let worker = db_workers::get_worker(&state.db, &id).await?;
    match worker {
        None => Err(AppError::NotFound(format!("Worker {} not found", id))),
        Some(_w) => {
            poller::poll_all_workers(&state.db, &state.http_client).await;
            Ok(Json(json!({ "polled": true })))
        }
    }
}
