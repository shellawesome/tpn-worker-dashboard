use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "tpn-worker-dashboard", about = "TPN Worker Dashboard — multi-worker management panel")]
pub struct DashboardConfig {
    #[clap(env = "DASHBOARD_PORT", default_value = "8080")]
    pub port: u16,

    #[clap(env = "DASHBOARD_SQLITE_PATH", default_value = "~/.config/tpn-worker-dashboard/dashboard.db")]
    pub sqlite_path: String,

    #[clap(env = "DASHBOARD_POLL_INTERVAL", default_value = "30")]
    pub poll_interval_seconds: u64,

    #[clap(env = "DASHBOARD_POLL_TIMEOUT", default_value = "10")]
    pub poll_timeout_seconds: u64,

    #[clap(env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[clap(env = "DASHBOARD_GITHUB_REPO", default_value = "shellawesome/tpn-worker-dashboard")]
    pub github_repo: String,
}
