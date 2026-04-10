use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;

use crate::AppState;

const ASSET_NAME: &str = "tpn-worker-dashboard-linux-ubuntu22-amd64";

#[derive(Serialize)]
struct VersionInfo {
    current: String,
    latest: Option<String>,
    has_update: bool,
    download_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// GET /api/version — Check current version and latest GitHub release.
pub async fn get_version(State(state): State<AppState>) -> impl IntoResponse {
    let current = format!("v{}", state.version);
    let repo = &state.config.github_repo;

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return Json(serde_json::json!({
                "success": true,
                "data": VersionInfo { current, latest: None, has_update: false, download_url: None }
            }));
        }
    };

    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let resp = client
        .get(&url)
        .header("User-Agent", "tpn-worker-dashboard")
        .send()
        .await;

    match resp {
        Ok(r) => {
            if let Ok(release) = r.json::<GitHubRelease>().await {
                let latest = release.tag_name.clone();
                let latest_clean = latest.trim_start_matches('v');
                let current_clean = current.trim_start_matches('v');
                let has_update = latest_clean != current_clean;

                let download_url = release
                    .assets
                    .iter()
                    .find(|a| a.name == ASSET_NAME)
                    .map(|a| a.browser_download_url.clone());

                Json(serde_json::json!({
                    "success": true,
                    "data": VersionInfo { current, latest: Some(latest), has_update, download_url }
                }))
            } else {
                Json(serde_json::json!({
                    "success": true,
                    "data": VersionInfo { current, latest: None, has_update: false, download_url: None }
                }))
            }
        }
        Err(_) => Json(serde_json::json!({
            "success": true,
            "data": VersionInfo { current, latest: None, has_update: false, download_url: None }
        })),
    }
}

fn err(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "success": false, "message": msg.into() })),
    )
}

/// POST /api/upgrade — Download latest release and replace binary, then restart.
pub async fn do_upgrade(State(state): State<AppState>) -> impl IntoResponse {
    let repo = &state.config.github_repo;

    let download_url = format!(
        "https://github.com/{}/releases/download/latest/{}",
        repo, ASSET_NAME
    );
    info!("Downloading dashboard update from: {}", download_url);

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(c) => c,
        Err(e) => return err(format!("创建下载客户端失败: {}", e)),
    };

    let binary_data = match client
        .get(&download_url)
        .header("User-Agent", "tpn-worker-dashboard")
        .send()
        .await
    {
        Ok(r) => {
            if !r.status().is_success() {
                return err(format!("下载失败, HTTP {}", r.status()));
            }
            match r.bytes().await {
                Ok(b) => b,
                Err(e) => return err(format!("读取下载数据失败: {}", e)),
            }
        }
        Err(e) => return err(format!("下载失败: {}", e)),
    };

    let temp_path = "/tmp/tpn-worker-dashboard-new";

    if let Err(e) = std::fs::write(temp_path, &binary_data) {
        return err(format!("写入临时文件失败: {}", e));
    }

    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(temp_path, std::fs::Permissions::from_mode(0o755)) {
        return err(format!("设置权限失败: {}", e));
    }

    // Verify new binary
    let verify = tokio::process::Command::new(temp_path)
        .arg("--help")
        .output()
        .await;
    if verify.is_err() {
        let _ = std::fs::remove_file(temp_path);
        return err("新二进制验证失败");
    }

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return err(format!("获取当前二进制路径失败: {}", e)),
    };

    let backup_path = format!("{}.bak", current_exe.display());
    if let Err(e) = std::fs::copy(&current_exe, &backup_path) {
        return err(format!("备份失败: {}", e));
    }

    // Replace: delete (allowed for running exe on Linux) then copy.
    if let Err(e) = std::fs::remove_file(&current_exe) {
        return err(format!("删除旧二进制失败: {}", e));
    }
    if let Err(e) = std::fs::copy(temp_path, &current_exe) {
        let _ = std::fs::copy(&backup_path, &current_exe);
        return err(format!("替换二进制失败: {}", e));
    }
    if let Err(e) =
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))
    {
        let _ = std::fs::remove_file(&current_exe);
        let _ = std::fs::copy(&backup_path, &current_exe);
        return err(format!("设置新二进制权限失败: {}", e));
    }
    let _ = std::fs::remove_file(temp_path);

    info!("Dashboard upgrade successful! Scheduling restart...");

    let exe_for_restart = current_exe.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("Restarting tpn-worker-dashboard via nohup...");
        let _ = tokio::process::Command::new("nohup")
            .arg(&exe_for_restart)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        std::process::exit(0);
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "message": "更新成功，正在重启..." })),
    )
}
