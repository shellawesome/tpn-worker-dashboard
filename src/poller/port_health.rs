use crate::db::port_health::{self, PortHealthRow};
use crate::db::{workers, DbPool};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{info, warn};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const UDP_WAIT: Duration = Duration::from_millis(800);

async fn tcp_check(host: &str, port: u16) -> bool {
    if host.is_empty() || port == 0 {
        return false;
    }
    let addr = format!("{}:{}", host, port);
    match timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

/// WireGuard handshake init packet (type 0x01) — 148 bytes of zeros after
/// the type byte. We don't need a valid handshake: WG silently drops invalid
/// packets, so we rely on ICMP port-unreachable detection (returned as
/// ConnectionRefused on the next recv) to determine "closed". Timeout or
/// silent success => assume open/reachable.
fn wg_handshake_init() -> Vec<u8> {
    let mut buf = vec![0u8; 148];
    buf[0] = 0x01;
    buf
}

async fn udp_check(host: &str, port: u16) -> bool {
    if host.is_empty() || port == 0 {
        return false;
    }
    let sock = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return false,
    };
    if sock.connect((host, port)).await.is_err() {
        return false;
    }
    let pkt = wg_handshake_init();
    if sock.send(&pkt).await.is_err() {
        return false;
    }
    // Try to receive: ConnectionRefused (from ICMP port unreachable) means
    // the port is closed. A timeout or a response means reachable.
    let mut buf = [0u8; 512];
    match timeout(UDP_WAIT, sock.recv(&mut buf)).await {
        Ok(Ok(_)) => true, // got a reply — definitely reachable
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => false,
        Ok(Err(_)) => false,
        Err(_) => true, // timeout — assume open (or filtered)
    }
}

fn host_from_url(url: &str) -> String {
    let s = url.trim();
    let after_scheme = s
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(s);
    let host_port = after_scheme.split('/').next().unwrap_or("");
    // strip user@
    let host_port = host_port.rsplit_once('@').map(|(_, h)| h).unwrap_or(host_port);
    // strip port
    if let Some(idx) = host_port.rfind(':') {
        // ipv6 [::]:port handling — if host starts with '[' take up to ']'
        if let Some(end) = host_port.find(']') {
            return host_port[..=end].to_string();
        }
        return host_port[..idx].to_string();
    }
    host_port.to_string()
}

pub async fn check_all_workers(pool: &DbPool) {
    let list = match workers::list_workers_with_latest(pool).await {
        Ok(w) => w,
        Err(e) => {
            warn!("port_health: failed to list workers: {}", e);
            return;
        }
    };

    let sem = Arc::new(Semaphore::new(20));
    let mut handles = Vec::new();

    for w in list {
        let worker_id = w["id"].as_str().unwrap_or("").to_string();
        if worker_id.is_empty() {
            continue;
        }
        let worker_url = w["url"].as_str().unwrap_or("").to_string();

        // Parse latest snapshot data for authoritative ports/host
        let mut host = String::new();
        let mut public_port: u16 = 0;
        let mut socks5_port: u16 = 0;
        let mut http_port: u16 = 0;
        let mut wg_port: u16 = 0;

        if let Some(latest) = w.get("latest") {
            if !latest.is_null() {
                // Need to fetch snapshot data JSON — fetch latest snapshot for this worker
                if let Ok(hist) =
                    crate::db::snapshots::worker_history(pool, &worker_id, 1, 0).await
                {
                    if let Some(row) = hist.first() {
                        let data_str = row.get("data").and_then(|v| v.as_str()).unwrap_or("");
                        if !data_str.is_empty() {
                            if let Ok(data) =
                                serde_json::from_str::<serde_json::Value>(data_str)
                            {
                                host = data["network"]["public_host"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                public_port = data["network"]["public_port"].as_u64().unwrap_or(0) as u16;
                                socks5_port = data["proxy"]["socks5_port"].as_u64().unwrap_or(0) as u16;
                                http_port = data["proxy"]["http_proxy_port"].as_u64().unwrap_or(0) as u16;
                                wg_port = data["wireguard"]["listen_port"].as_u64().unwrap_or(0) as u16;
                            }
                        }
                    }
                }
            }
        }

        if host.is_empty() {
            host = host_from_url(&worker_url);
        }

        let pool = pool.clone();
        let sem = sem.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let (pub_ok, s5_ok, http_ok, wg_ok) = tokio::join!(
                tcp_check(&host, public_port),
                tcp_check(&host, socks5_port),
                tcp_check(&host, http_port),
                udp_check(&host, wg_port),
            );

            let row = PortHealthRow {
                worker_id: worker_id.clone(),
                checked_at: chrono::Utc::now().to_rfc3339(),
                host: host.clone(),
                public_port: public_port as i32,
                public_ok: pub_ok,
                socks5_port: socks5_port as i32,
                socks5_ok: s5_ok,
                http_port: http_port as i32,
                http_ok: http_ok,
                wg_port: wg_port as i32,
                wg_ok: wg_ok,
            };
            if let Err(e) = port_health::upsert(&pool, &row).await {
                warn!("port_health: upsert failed for {}: {}", worker_id, e);
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

pub async fn run_port_health_loop(pool: DbPool, interval_secs: u64) {
    let interval = Duration::from_secs(interval_secs);
    let mut ticker = tokio::time::interval(interval);
    // Do an immediate first tick so we get data shortly after startup.
    loop {
        ticker.tick().await;
        info!("Running port health TCP checks...");
        check_all_workers(&pool).await;
    }
}
