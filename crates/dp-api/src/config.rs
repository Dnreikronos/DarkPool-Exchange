use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(name = "darkpool-server", about = "Dark Pool Exchange operator API server")]
pub struct Config {
    #[arg(long, env = "DARKPOOL_GRPC_ADDR", default_value = "0.0.0.0:9090")]
    pub grpc_addr: SocketAddr,

    #[arg(long, env = "DARKPOOL_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: SocketAddr,

    #[arg(long, env = "DARKPOOL_AUCTION_INTERVAL", default_value = "5s", value_parser = parse_duration)]
    pub auction_interval: Duration,

    #[arg(long, env = "DARKPOOL_API_KEYS", default_value = "")]
    api_keys_raw: String,

    #[arg(long, env = "DARKPOOL_RATE_LIMIT", default_value = "10")]
    pub rate_limit: f64,

    #[arg(long, env = "DARKPOOL_RATE_BURST", default_value = "20")]
    pub rate_burst: f64,

    #[arg(long, env = "DARKPOOL_RATE_STALE_AFTER", default_value = "10m", value_parser = parse_duration)]
    pub rate_stale_after: Duration,

    #[arg(long, env = "DARKPOOL_EVENT_LOG", default_value = "")]
    pub event_log: String,

    #[arg(long, env = "DARKPOOL_OPERATOR_KEY", default_value = "")]
    pub operator_key: String,

    #[arg(long, env = "DARKPOOL_AGGREGATOR_BIN", default_value = "")]
    pub aggregator_bin: String,

    #[arg(long, env = "DARKPOOL_AGGREGATOR_TIMEOUT", default_value = "30s", value_parser = parse_duration)]
    pub aggregator_timeout: Duration,

    /// On-chain submission deadline. Falls back to aggregator_timeout when 0.
    #[arg(long, env = "DARKPOOL_SUBMIT_TIMEOUT", default_value = "0s", value_parser = parse_duration)]
    pub submit_timeout: Duration,

    #[arg(long, env = "DARKPOOL_ETH_RPC", default_value = "")]
    pub eth_rpc: String,

    #[arg(long, env = "DARKPOOL_CONTRACT_ADDR", default_value = "")]
    pub contract_addr: String,

    #[arg(long, env = "DARKPOOL_CHAIN_ID", default_value = "0")]
    pub chain_id: u64,

    #[arg(long, env = "DARKPOOL_SUBMIT_GAS", default_value = "500000")]
    pub submit_gas: u64,
}

impl Config {
    pub fn api_keys(&self) -> Vec<String> {
        self.api_keys_raw
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("invalid duration {}: {}", s, e))
}
