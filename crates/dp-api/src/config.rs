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

    /// PEM-encoded TLS server certificate (chain). When set together
    /// with --tls-key, TLS is enabled on both gRPC and REST listeners.
    /// When both are empty, both listeners bind plaintext (see the TLS
    /// runbook at docs/operations/tls-setup.md).
    #[arg(long, env = "DARKPOOL_TLS_CERT", default_value = "")]
    pub tls_cert: String,

    /// PEM-encoded TLS private key matching --tls-cert. Must be readable
    /// only by the service user (mode 0400 in prod). Unset together with
    /// --tls-cert means plaintext.
    #[arg(long, env = "DARKPOOL_TLS_KEY", default_value = "")]
    pub tls_key: String,

    /// PEM-encoded CA bundle used to verify *client* certificates.
    /// Presence enables mTLS on both listeners — clients without a cert
    /// signed by this CA are rejected at the TLS handshake. Requires
    /// --tls-cert / --tls-key.
    #[arg(long, env = "DARKPOOL_TLS_CLIENT_CA", default_value = "")]
    pub tls_client_ca: String,

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

    /// Enable / disable the periodic state-snapshot task. Enabled by
    /// default — disable only when the operator wants a pure
    /// event-replay recovery path (e.g. for forensic reproduction of
    /// historical state).
    #[arg(long, env = "DARKPOOL_SNAPSHOT_ENABLED", default_value = "true")]
    pub snapshot_enabled: bool,

    /// Directory (file backend) or empty (DB backend / memory) for the
    /// snapshot store. Required when `--event-log` is in use and
    /// snapshots are enabled. Ignored for the postgres backend, which
    /// stores snapshots in the same DB as events.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_DIR", default_value = "")]
    pub snapshot_dir: String,

    /// Snapshot whenever this many events have accrued since the last
    /// snapshot. Lower values shorten replay time at the cost of more
    /// writes; defaults to 10k which keeps cold-start under a second
    /// for the in-memory engine.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_EVERY_EVENTS", default_value = "10000")]
    pub snapshot_every_events: u64,

    /// Force a snapshot if this long has elapsed even when the
    /// event-count threshold has not been crossed.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_INTERVAL", default_value = "300s", value_parser = parse_duration)]
    pub snapshot_interval: Duration,

    /// After a snapshot is written, compact event-log entries whose
    /// seq is at least this far behind the new snapshot's seq. Keeping
    /// a tail buys forensic visibility into the most recent activity
    /// even after compaction.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_RETAIN_EVENTS", default_value = "1024")]
    pub snapshot_retain_events: u64,

    /// Number of snapshot envelopes to keep in the store. Older
    /// snapshots are pruned via `SnapshotStore::delete_before`.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_RETAIN_COUNT", default_value = "3")]
    pub snapshot_retain_count: usize,

    #[arg(long, env = "DARKPOOL_OPERATOR_KEY", default_value = "")]
    pub operator_key: String,

    /// Comma-separated list of ECIES key URIs for the multi-key
    /// decrypter, with optional `@active|@rotating|@sunset` status
    /// suffix (defaults to `@active`). Example:
    /// `file:/etc/dp/active.hex@active,age:/etc/dp/old.age@sunset`.
    /// When set, takes precedence over `DARKPOOL_OPERATOR_KEY`.
    #[arg(long, env = "DARKPOOL_OPERATOR_KEY_URIS", default_value = "")]
    pub operator_key_uris: String,

    /// URI for the Ethereum transaction signer. Independent from the
    /// ECIES decryption key — settlement and order-decryption secrets
    /// are intentionally separate. Schemes: `file:`, `age:`, `awskms:`.
    /// When unset, falls back to the noop submitter even if
    /// `DARKPOOL_ETH_RPC` is configured.
    #[arg(long, env = "DARKPOOL_SIGNER_KEY_URI", default_value = "")]
    pub signer_key_uri: String,

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

    pub fn snapshot_dir_path(&self) -> Option<&str> {
        opt(&self.snapshot_dir)
    }

    pub fn operator_key_path(&self) -> Option<&str> {
        opt(&self.operator_key)
    }

    pub fn operator_key_uris_str(&self) -> Option<&str> {
        opt(&self.operator_key_uris)
    }

    pub fn signer_key_uri_str(&self) -> Option<&str> {
        opt(&self.signer_key_uri)
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

    pub fn tls_cert_path(&self) -> Option<&str> {
        opt(&self.tls_cert)
    }

    pub fn tls_key_path(&self) -> Option<&str> {
        opt(&self.tls_key)
    }

    pub fn tls_client_ca_path(&self) -> Option<&str> {
        opt(&self.tls_client_ca)
    }

    /// Resolve the requested TLS posture, rejecting half-configured
    /// states (cert without key or vice-versa). Caller should error
    /// out at boot when this returns `Err`.
    pub fn tls_mode(&self) -> Result<TlsMode, String> {
        match (
            self.tls_cert_path(),
            self.tls_key_path(),
            self.tls_client_ca_path(),
        ) {
            (None, None, None) => Ok(TlsMode::Plaintext),
            (Some(c), Some(k), None) => Ok(TlsMode::Tls {
                cert: c.into(),
                key: k.into(),
            }),
            (Some(c), Some(k), Some(ca)) => Ok(TlsMode::Mtls {
                cert: c.into(),
                key: k.into(),
                client_ca: ca.into(),
            }),
            (Some(_), None, _) => {
                Err("--tls-cert / DARKPOOL_TLS_CERT set but --tls-key is missing".into())
            }
            (None, Some(_), _) => {
                Err("--tls-key / DARKPOOL_TLS_KEY set but --tls-cert is missing".into())
            }
            (None, None, Some(_)) => Err(
                "--tls-client-ca / DARKPOOL_TLS_CLIENT_CA set without --tls-cert and --tls-key"
                    .into(),
            ),
        }
    }
}

/// Resolved TLS posture for both listeners. The variants are exhaustive
/// of what the operator can ask for via [`Config::tls_mode`]:
///
/// - `Plaintext` — no TLS material configured; both listeners bind
///   plaintext. Acceptable for local dev only; main.rs logs a loud
///   warning at boot.
/// - `Tls` — server-auth TLS, no client cert required.
/// - `Mtls` — mutual TLS, clients must present a cert signed by
///   `client_ca`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsMode {
    Plaintext,
    Tls {
        cert: std::path::PathBuf,
        key: std::path::PathBuf,
    },
    Mtls {
        cert: std::path::PathBuf,
        key: std::path::PathBuf,
        client_ca: std::path::PathBuf,
    },
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

    fn cfg_with_tls(cert: &str, key: &str, ca: &str) -> Config {
        // Parser preserves the env defaults for everything else; we only
        // care about the TLS triple here.
        Config::parse_from([
            "darkpool-server",
            "--tls-cert",
            cert,
            "--tls-key",
            key,
            "--tls-client-ca",
            ca,
        ])
    }

    #[test]
    fn tls_mode_plaintext_when_empty() {
        let cfg = cfg_with_tls("", "", "");
        assert_eq!(cfg.tls_mode().unwrap(), TlsMode::Plaintext);
    }

    #[test]
    fn tls_mode_tls_when_cert_and_key() {
        let cfg = cfg_with_tls("/srv/cert.pem", "/srv/key.pem", "");
        match cfg.tls_mode().unwrap() {
            TlsMode::Tls { cert, key } => {
                assert_eq!(cert.to_str(), Some("/srv/cert.pem"));
                assert_eq!(key.to_str(), Some("/srv/key.pem"));
            }
            other => panic!("expected Tls, got {other:?}"),
        }
    }

    #[test]
    fn tls_mode_mtls_when_full_triple() {
        let cfg = cfg_with_tls("/srv/cert.pem", "/srv/key.pem", "/srv/ca.pem");
        match cfg.tls_mode().unwrap() {
            TlsMode::Mtls {
                cert,
                key,
                client_ca,
            } => {
                assert_eq!(cert.to_str(), Some("/srv/cert.pem"));
                assert_eq!(key.to_str(), Some("/srv/key.pem"));
                assert_eq!(client_ca.to_str(), Some("/srv/ca.pem"));
            }
            other => panic!("expected Mtls, got {other:?}"),
        }
    }

    #[test]
    fn tls_mode_rejects_cert_without_key() {
        let cfg = cfg_with_tls("/srv/cert.pem", "", "");
        let err = cfg.tls_mode().unwrap_err();
        assert!(err.contains("tls-key"), "msg: {err}");
    }

    #[test]
    fn tls_mode_rejects_key_without_cert() {
        let cfg = cfg_with_tls("", "/srv/key.pem", "");
        let err = cfg.tls_mode().unwrap_err();
        assert!(err.contains("tls-cert"), "msg: {err}");
    }

    #[test]
    fn tls_mode_rejects_client_ca_without_server_material() {
        // Client-CA on its own would silently downgrade to plaintext if
        // we didn't reject it — operators would think mTLS is on.
        let cfg = cfg_with_tls("", "", "/srv/ca.pem");
        let err = cfg.tls_mode().unwrap_err();
        assert!(err.contains("tls-client-ca"), "msg: {err}");
    }
}
