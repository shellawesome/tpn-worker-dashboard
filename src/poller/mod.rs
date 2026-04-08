use crate::db::{snapshots, workers, DbPool};
use crate::models::WorkerDashboardResponse;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// Poll all enabled workers and store snapshots.
pub async fn poll_all_workers(pool: &DbPool, client: &reqwest::Client) {
    let workers = match workers::list_workers_with_latest(pool).await {
        Ok(w) => w,
        Err(e) => {
            warn!("Failed to list workers for polling: {}", e);
            return;
        }
    };

    let sem = Arc::new(Semaphore::new(20));
    let mut handles = Vec::new();

    for w in &workers {
        let poll_enabled = w.get("poll_enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        if !poll_enabled {
            continue;
        }

        let worker_id = w["id"].as_str().unwrap_or("").to_string();
        let worker_url = w["url"].as_str().unwrap_or("").to_string();
        let api_key = w["api_key"].as_str().unwrap_or("").to_string();
        if worker_url.is_empty() {
            continue;
        }

        let pool = pool.clone();
        let client = client.clone();
        let sem = sem.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            poll_single_worker(&pool, &client, &worker_id, &worker_url, &api_key).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

/// Login to worker and get JWT token. Returns None if no auth needed.
async fn get_auth_token(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Option<String>, String> {
    if api_key.is_empty() {
        return Ok(None);
    }

    let url = format!("{}/api/login", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(10))
        .json(&serde_json::json!({ "password": api_key }))
        .send()
        .await
        .map_err(|e| format!("Login failed: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Login parse error: {}", e))?;

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

async fn poll_single_worker(
    pool: &DbPool,
    client: &reqwest::Client,
    worker_id: &str,
    base_url: &str,
    api_key: &str,
) {
    let base = base_url.trim_end_matches('/');
    let now = chrono::Utc::now().to_rfc3339();
    let start = Instant::now();

    // Authenticate if needed
    let token = match get_auth_token(client, base, api_key).await {
        Ok(t) => t,
        Err(e) => {
            let latency = start.elapsed().as_millis() as i64;
            let _ = snapshots::insert_snapshot(
                pool, worker_id, &now, latency, false,
                &format!("Auth error: {}", e), "", "", "", 0, false, 0, 0, 0, 0,
            ).await;
            return;
        }
    };

    let url = format!("{}/api/dashboard", base);
    let mut req = client.get(&url);
    if let Some(ref t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }

    match req.send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as i64;
            let status = resp.status();
            match resp.text().await {
                Ok(body) => {
                    if !status.is_success() {
                        let _ = snapshots::insert_snapshot(
                            pool, worker_id, &now, latency, false,
                            &format!("HTTP {}", status), "", "", "", 0, false, 0, 0, 0, 0,
                        ).await;
                        return;
                    }

                    match serde_json::from_str::<WorkerDashboardResponse>(&body) {
                        Ok(data) => {
                            let _ = snapshots::insert_snapshot(
                                pool,
                                worker_id,
                                &now,
                                latency,
                                true,
                                "",
                                &body,
                                &data.worker.version,
                                &data.worker.mode,
                                data.worker.uptime_seconds,
                                data.mining_pool.last_registration_success,
                                data.wireguard.active_peers,
                                data.wireguard.max_peers,
                                data.proxy.available_non_priority,
                                data.proxy.credential_count,
                            )
                            .await;
                        }
                        Err(e) => {
                            let _ = snapshots::insert_snapshot(
                                pool, worker_id, &now, latency, false,
                                &format!("JSON parse error: {}", e), &body,
                                "", "", 0, false, 0, 0, 0, 0,
                            ).await;
                        }
                    }
                }
                Err(e) => {
                    let latency = start.elapsed().as_millis() as i64;
                    let _ = snapshots::insert_snapshot(
                        pool, worker_id, &now, latency, false,
                        &format!("Read body error: {}", e), "", "", "", 0, false, 0, 0, 0, 0,
                    ).await;
                }
            }
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as i64;
            let _ = snapshots::insert_snapshot(
                pool, worker_id, &now, latency, false,
                &e.to_string(), "", "", "", 0, false, 0, 0, 0, 0,
            ).await;
        }
    }
}

/// Run the polling loop.
pub async fn run_poller(pool: DbPool, client: reqwest::Client, interval_secs: u64) {
    let interval = Duration::from_secs(interval_secs);
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip first immediate tick

    loop {
        ticker.tick().await;
        info!("Polling all workers...");
        poll_all_workers(&pool, &client).await;
    }
}
