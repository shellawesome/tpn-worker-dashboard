use axum::extract::State;
use axum::response::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::db::workers as db_workers;
use crate::error::AppError;
use crate::AppState;

// ── Request types ──

#[derive(Deserialize)]
pub struct BatchRequest {
    pub worker_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct BatchConfigRequest {
    pub worker_ids: Vec<String>,
    pub mining_pool_url: Option<String>,
    pub payment_address_evm: Option<String>,
    pub payment_address_bittensor: Option<String>,
}

#[derive(Deserialize)]
pub struct BatchPasswordRequest {
    pub worker_ids: Vec<String>,
    pub password: String,
}

// ── Auth helper ──

/// Login to a worker and get JWT token. Returns None if no password needed.
async fn get_auth_token(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Option<String>, String> {
    if api_key.is_empty() {
        return Ok(None);
    }

    let url = format!("{}/api/login", base_url);
    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(10))
        .json(&json!({ "password": api_key }))
        .send()
        .await
        .map_err(|e| format!("Login failed: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Login response parse error: {}", e))?;

    if body["success"].as_bool() == Some(true) {
        if let Some(token) = body["data"]["token"].as_str() {
            return Ok(Some(token.to_string()));
        }
    }

    Err(body["message"]
        .as_str()
        .unwrap_or("Login failed")
        .to_string())
}

/// Build an authenticated GET request.
async fn authed_get(
    client: &reqwest::Client,
    url: &str,
    token: &Option<String>,
    timeout: Duration,
) -> Result<Value, String> {
    let mut req = client.get(url).timeout(timeout);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }
    Ok(body)
}

/// Build an authenticated POST request with optional JSON body.
async fn authed_post(
    client: &reqwest::Client,
    url: &str,
    token: &Option<String>,
    body: Option<Value>,
    timeout: Duration,
) -> Result<Value, String> {
    let mut req = client.post(url).timeout(timeout);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body["message"]
            .as_str()
            .or(body["error"].as_str())
            .unwrap_or("unknown error");
        return Err(format!("HTTP {}: {}", status, msg));
    }
    Ok(body)
}

// ── Batch handlers ──

/// POST /api/batch/version — Check version on multiple workers.
pub async fn batch_check_version(
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, AppError> {
    let workers = db_workers::get_workers_by_ids(&state.db, &body.worker_ids).await?;
    if workers.is_empty() {
        return Err(AppError::BadRequest("No valid workers found".into()));
    }

    let sem = Arc::new(Semaphore::new(20));
    let client = state.http_client.clone();
    let mut handles = Vec::new();

    for w in &workers {
        let sem = sem.clone();
        let client = client.clone();
        let id = w["id"].as_str().unwrap_or("").to_string();
        let name = w["name"].as_str().unwrap_or("").to_string();
        let url = w["url"].as_str().unwrap_or("").trim_end_matches('/').to_string();
        let api_key = w["api_key"].as_str().unwrap_or("").to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let token = match get_auth_token(&client, &url, &api_key).await {
                Ok(t) => t,
                Err(e) => {
                    return json!({ "worker_id": id, "worker_name": name, "success": false, "message": e });
                }
            };

            let api_url = format!("{}/api/version", url);
            match authed_get(&client, &api_url, &token, Duration::from_secs(10)).await {
                Ok(data) => {
                    let ver_data = &data["data"];
                    json!({
                        "worker_id": id,
                        "worker_name": name,
                        "success": true,
                        "message": format!("{} → {}{}",
                            ver_data["current"].as_str().unwrap_or("-"),
                            ver_data["latest"].as_str().unwrap_or("-"),
                            if ver_data["has_update"].as_bool() == Some(true) { " (update available)" } else { "" }
                        ),
                        "data": ver_data.clone()
                    })
                }
                Err(e) => {
                    json!({ "worker_id": id, "worker_name": name, "success": false, "message": e })
                }
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }

    Ok(Json(json!({ "results": results })))
}

/// POST /api/batch/upgrade — Upgrade multiple workers.
pub async fn batch_upgrade(
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, AppError> {
    let workers = db_workers::get_workers_by_ids(&state.db, &body.worker_ids).await?;
    if workers.is_empty() {
        return Err(AppError::BadRequest("No valid workers found".into()));
    }

    let sem = Arc::new(Semaphore::new(20));
    let client = state.http_client.clone();
    let mut handles = Vec::new();

    for w in &workers {
        let sem = sem.clone();
        let client = client.clone();
        let id = w["id"].as_str().unwrap_or("").to_string();
        let name = w["name"].as_str().unwrap_or("").to_string();
        let url = w["url"].as_str().unwrap_or("").trim_end_matches('/').to_string();
        let api_key = w["api_key"].as_str().unwrap_or("").to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let token = match get_auth_token(&client, &url, &api_key).await {
                Ok(t) => t,
                Err(e) => {
                    return json!({ "worker_id": id, "worker_name": name, "success": false, "message": e });
                }
            };

            let api_url = format!("{}/api/upgrade", url);
            match authed_post(&client, &api_url, &token, None, Duration::from_secs(120)).await {
                Ok(data) => {
                    let msg = data["message"].as_str().unwrap_or("OK").to_string();
                    json!({ "worker_id": id, "worker_name": name, "success": true, "message": msg })
                }
                Err(e) => {
                    json!({ "worker_id": id, "worker_name": name, "success": false, "message": e })
                }
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }

    Ok(Json(json!({ "results": results })))
}

/// POST /api/batch/config — Update config on multiple workers.
pub async fn batch_config(
    State(state): State<AppState>,
    Json(body): Json<BatchConfigRequest>,
) -> Result<Json<Value>, AppError> {
    if body.mining_pool_url.is_none()
        && body.payment_address_evm.is_none()
        && body.payment_address_bittensor.is_none()
    {
        return Err(AppError::BadRequest("At least one config field is required".into()));
    }

    let workers = db_workers::get_workers_by_ids(&state.db, &body.worker_ids).await?;
    if workers.is_empty() {
        return Err(AppError::BadRequest("No valid workers found".into()));
    }

    // Build the payload to send to each worker
    let mut payload = serde_json::Map::new();
    if let Some(ref v) = body.mining_pool_url {
        payload.insert("mining_pool_url".into(), json!(v));
    }
    if let Some(ref v) = body.payment_address_evm {
        payload.insert("payment_address_evm".into(), json!(v));
    }
    if let Some(ref v) = body.payment_address_bittensor {
        payload.insert("payment_address_bittensor".into(), json!(v));
    }
    let payload = Value::Object(payload);

    let sem = Arc::new(Semaphore::new(20));
    let client = state.http_client.clone();
    let mut handles = Vec::new();

    for w in &workers {
        let sem = sem.clone();
        let client = client.clone();
        let payload = payload.clone();
        let id = w["id"].as_str().unwrap_or("").to_string();
        let name = w["name"].as_str().unwrap_or("").to_string();
        let url = w["url"].as_str().unwrap_or("").trim_end_matches('/').to_string();
        let api_key = w["api_key"].as_str().unwrap_or("").to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let token = match get_auth_token(&client, &url, &api_key).await {
                Ok(t) => t,
                Err(e) => {
                    return json!({ "worker_id": id, "worker_name": name, "success": false, "message": e });
                }
            };

            let api_url = format!("{}/api/config/update", url);
            match authed_post(&client, &api_url, &token, Some(payload), Duration::from_secs(10)).await {
                Ok(data) => {
                    let msg = data["message"].as_str().unwrap_or("OK").to_string();
                    json!({ "worker_id": id, "worker_name": name, "success": true, "message": msg })
                }
                Err(e) => {
                    json!({ "worker_id": id, "worker_name": name, "success": false, "message": e })
                }
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }

    Ok(Json(json!({ "results": results })))
}

/// POST /api/batch/password — Set password on multiple workers.
pub async fn batch_set_password(
    State(state): State<AppState>,
    Json(body): Json<BatchPasswordRequest>,
) -> Result<Json<Value>, AppError> {
    if body.password.len() < 4 {
        return Err(AppError::BadRequest("Password must be at least 4 characters".into()));
    }

    let workers = db_workers::get_workers_by_ids(&state.db, &body.worker_ids).await?;
    if workers.is_empty() {
        return Err(AppError::BadRequest("No valid workers found".into()));
    }

    let sem = Arc::new(Semaphore::new(20));
    let client = state.http_client.clone();
    let db = state.db.clone();
    let mut handles = Vec::new();

    for w in &workers {
        let sem = sem.clone();
        let client = client.clone();
        let db = db.clone();
        let password = body.password.clone();
        let id = w["id"].as_str().unwrap_or("").to_string();
        let name = w["name"].as_str().unwrap_or("").to_string();
        let url = w["url"].as_str().unwrap_or("").trim_end_matches('/').to_string();
        let api_key = w["api_key"].as_str().unwrap_or("").to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let token = match get_auth_token(&client, &url, &api_key).await {
                Ok(t) => t,
                Err(e) => {
                    return json!({ "worker_id": id, "worker_name": name, "success": false, "message": e });
                }
            };

            let api_url = format!("{}/api/password", url);
            let payload = json!({ "password": password });
            match authed_post(&client, &api_url, &token, Some(payload), Duration::from_secs(10)).await {
                Ok(data) => {
                    // Update api_key in dashboard DB so future auth uses new password
                    let _ = db_workers::update_worker(&db, &id, None, None, None, Some(&password), None).await;
                    let msg = data["message"].as_str().unwrap_or("OK").to_string();
                    json!({ "worker_id": id, "worker_name": name, "success": true, "message": msg })
                }
                Err(e) => {
                    json!({ "worker_id": id, "worker_name": name, "success": false, "message": e })
                }
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }

    Ok(Json(json!({ "results": results })))
}
