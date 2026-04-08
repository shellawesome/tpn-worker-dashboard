use serde::Deserialize;

/// Matches tpn-worker's GET /api/dashboard response.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WorkerDashboardResponse {
    pub worker: WorkerInfo,
    pub mining_pool: MiningPoolInfo,
    pub wireguard: WireguardInfo,
    pub proxy: ProxyInfo,
    pub network: NetworkInfo,
    pub payment: PaymentInfo,
    pub database: DatabaseInfo,
    #[serde(default)]
    pub registration_history: Vec<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WorkerInfo {
    pub version: String,
    pub mode: String,
    pub uptime_seconds: i64,
    pub start_time: String,
    pub git_branch: String,
    pub git_hash: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MiningPoolInfo {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub last_registration_success: bool,
    #[serde(default)]
    pub last_registration_time: String,
    #[serde(default)]
    pub rewards_url: String,
    #[serde(default)]
    pub website_url: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WireguardInfo {
    pub enabled: bool,
    #[serde(default)]
    pub interface: String,
    #[serde(default)]
    pub max_peers: i32,
    #[serde(default)]
    pub active_peers: i32,
    #[serde(default)]
    pub active_leases: i32,
    #[serde(default)]
    pub listen_port: u16,
    #[serde(default)]
    pub subnet: String,
    #[serde(default)]
    pub dns: String,
    #[serde(default)]
    pub server_public_key: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ProxyInfo {
    #[serde(default)]
    pub socks5_port: u16,
    #[serde(default)]
    pub http_proxy_port: u16,
    #[serde(default)]
    pub credential_count: i32,
    #[serde(default)]
    pub active_credentials: i32,
    #[serde(default)]
    pub available_non_priority: i64,
    #[serde(default)]
    pub priority_slots: i64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct NetworkInfo {
    #[serde(default)]
    pub public_host: String,
    #[serde(default)]
    pub public_port: u16,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub base_url: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PaymentInfo {
    #[serde(default)]
    pub evm_address: String,
    #[serde(default)]
    pub bittensor_address: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DatabaseInfo {
    #[serde(default)]
    pub path: String,
}
