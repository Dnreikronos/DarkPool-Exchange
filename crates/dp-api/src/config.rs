use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(
    name = "darkpool-server",
    about = "Dark Pool Exchange operator API server"
)]
pub struct Config {
    #[arg(long, env = "DARKPOOL_GRPC_ADDR", default_value = "0.0.0.0:9090")]
    pub grpc_addr: SocketAddr,

    #[arg(long, env = "DARKPOOL_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: SocketAddr,

    #[arg(long, env = "DARKPOOL_AUCTION_INTERVAL", default_value = "5s", value_parser = parse_duration)]
    pub auction_interval: Duration,

    #[arg(long, env = "DARKPOOL_API_KEYS", default_value = "")]
    api_keys_raw: String,

    /// Comma-separated set of API keys with operator-admin scope. These
    /// keys are checked on `/v1/admin/*` paths instead of the public
    /// `DARKPOOL_API_KEYS`. Empty disables admin authentication entirely
    /// — fine for dev, never set this empty in production.
    #[arg(long, env = "DARKPOOL_OPERATOR_API_KEYS", default_value = "")]
    operator_api_keys_raw: String,

    /// JSON document seeding the pair registry on first boot. Only
    /// applied when the event log is empty (otherwise pairs are replayed
    /// from `PairRegistered` events). Format:
    /// `[{"pair":"ETH/USDC","baseToken":"0x...","quoteToken":"0x...","minOrderSize":"0.01","tickSize":"0.01"}]`.
    #[arg(long, env = "DARKPOOL_PAIR_SEED_JSON", default_value = "")]
    pub pair_seed_json: String,

    #[arg(long, env = "DARKPOOL_RATE_LIMIT", default_value = "10")]
    pub rate_limit: f64,

    #[arg(long, env = "DARKPOOL_RATE_BURST", default_value = "20")]
    pub rate_burst: f64,

    #[arg(long, env = "DARKPOOL_RATE_STALE_AFTER", default_value = "10m", value_parser = parse_duration)]
    pub rate_stale_after: Duration,

    #[arg(long, env = "DARKPOOL_EVENT_LOG", default_value = "")]
    pub event_log: String,

    #[arg(long, env = "DARKPOOL_EVENT_DB", default_value = "")]
    pub event_db: String,

    #[arg(long, env = "DARKPOOL_OPERATOR_KEY", default_value = "")]
    pub operator_key: String,

    #[arg(long, env = "DARKPOOL_AGGREGATOR_BIN", default_value = "")]
    pub aggregator_bin: String,

    #[arg(long, env = "DARKPOOL_AGGREGATOR_TIMEOUT", default_value = "30s", value_parser = parse_duration)]
    pub aggregator_timeout: Duration,

    /// Directory containing proving_key.bin / verifying_key.bin /
    /// keys_metadata.json. Forwarded to the aggregator subprocess via
    /// DARKPOOL_ZK_PROVING_KEY.
    #[arg(long, env = "DARKPOOL_ZK_PROVING_KEY", default_value = "")]
    pub zk_proving_key: String,

    /// Circuit batch size. Must equal the keygen-time value.
    #[arg(long, env = "DARKPOOL_ZK_BATCH_SIZE", default_value = "8")]
    pub zk_batch_size: u32,

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

    pub fn operator_api_keys(&self) -> Vec<String> {
        self.operator_api_keys_raw
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }

    pub fn pair_seed_json_str(&self) -> Option<&str> {
        opt(&self.pair_seed_json)
    }

    pub fn event_db_url(&self) -> Option<&str> {
        opt(&self.event_db)
    }

    pub fn event_log_path(&self) -> Option<&str> {
        opt(&self.event_log)
    }

    pub fn operator_key_path(&self) -> Option<&str> {
        opt(&self.operator_key)
    }

    pub fn aggregator_bin_path(&self) -> Option<&str> {
        opt(&self.aggregator_bin)
    }

    pub fn zk_proving_key_dir(&self) -> Option<&str> {
        opt(&self.zk_proving_key)
    }

    pub fn eth_rpc_url(&self) -> Option<&str> {
        opt(&self.eth_rpc)
    }

    pub fn contract_address(&self) -> Option<&str> {
        opt(&self.contract_addr)
    }
}

fn opt(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("invalid duration {}: {}", s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_returns_none_for_empty() {
        assert!(opt("").is_none());
        assert!(opt("   ").is_none());
    }

    #[test]
    fn opt_returns_trimmed_value() {
        assert_eq!(opt("  hello  "), Some("hello"));
        assert_eq!(opt("value"), Some("value"));
    }

    #[test]
    fn parse_duration_valid() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse_duration("2m30s").unwrap(), Duration::from_secs(150));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("not-a-duration").is_err());
    }
}
