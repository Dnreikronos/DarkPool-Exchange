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

    /// Permit binding plaintext (no TLS) on a non-loopback address.
    /// Without it, a plaintext bind to anything other than a loopback
    /// interface is a hard boot failure: the API key, the SIWE bearer
    /// token, and order ciphertext/metadata would otherwise travel in
    /// the clear to any on-path observer, defeating the SIWE replay
    /// mitigations that assume TLS. Loopback-only plaintext (local dev)
    /// never needs this flag. Set it only for trusted-network
    /// development or when a TLS-terminating reverse proxy / load
    /// balancer fronts the service (in which case also set
    /// --trusted-proxies). Never point a plaintext listener directly at
    /// an untrusted network. See [`Config::validate_plaintext_bind`].
    #[arg(long, env = "DARKPOOL_INSECURE", default_value = "false")]
    pub insecure: bool,

    #[arg(long, env = "DARKPOOL_AUCTION_INTERVAL", default_value = "5s", value_parser = parse_duration)]
    pub auction_interval: Duration,

    #[arg(long, env = "DARKPOOL_API_KEYS", default_value = "")]
    api_keys_raw: String,

    /// Comma-separated set of API keys with operator-admin scope. These
    /// keys are checked on `/v1/admin/*` paths instead of the public
    /// `DARKPOOL_API_KEYS`. An empty set is a hard boot failure
    /// (fail-closed) unless `--allow-unauthenticated-admin` is set —
    /// otherwise the admin router, including ECIES key rotation, would
    /// authenticate every request. See [`Config::validate_admin_auth`].
    #[arg(long, env = "DARKPOOL_OPERATOR_API_KEYS", default_value = "")]
    operator_api_keys_raw: String,

    /// Permit booting with an empty operator-admin key set. Without this
    /// flag an empty `DARKPOOL_OPERATOR_API_KEYS` aborts boot, because
    /// the admin router would otherwise accept unauthenticated requests
    /// (the "empty = allow all" rule in [`crate::auth::AuthCore::check`]).
    /// Local dev only — never set in production.
    #[arg(
        long,
        env = "DARKPOOL_ALLOW_UNAUTHENTICATED_ADMIN",
        default_value = "false"
    )]
    pub allow_unauthenticated_admin: bool,

    /// Comma-separated list of allowed CORS origins. When empty, no CORS
    /// headers are emitted (browser cross-origin requests will fail).
    /// Example: `http://localhost:3000,https://app.darkpool.exchange`.
    #[arg(long, env = "DARKPOOL_CORS_ORIGINS", default_value = "")]
    cors_origins_raw: String,

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

    /// Per-request handler timeout applied to REST and gRPC calls. Also caps
    /// gRPC auction stream lifetime; REST streaming responses are guarded
    /// separately by the SSE stream cap.
    #[arg(long, env = "DARKPOOL_REQUEST_TIMEOUT", default_value = "30s", value_parser = parse_duration)]
    pub request_timeout: Duration,

    /// Maximum number of request handlers running at once across both
    /// listeners. This is a process-wide cap, not a per-connection cap.
    #[arg(long, env = "DARKPOOL_MAX_CONCURRENT_REQUESTS", default_value = "1024")]
    pub max_concurrent_requests: usize,

    /// Maximum concurrent HTTP/2 streams accepted on each listener
    /// connection. gRPC is HTTP/2-only; the REST listener also applies this
    /// when clients negotiate h2.
    #[arg(
        long,
        env = "DARKPOOL_HTTP2_MAX_CONCURRENT_STREAMS",
        default_value = "128"
    )]
    pub http2_max_concurrent_streams: u32,

    /// Maximum simultaneously open auction SSE streams per resolved client
    /// key. Key derivation mirrors the request rate limiter.
    #[arg(long, env = "DARKPOOL_SSE_STREAMS_PER_KEY", default_value = "4")]
    pub sse_streams_per_key: usize,

    /// Comma/whitespace-separated CIDRs or IPs of trusted reverse
    /// proxies / load balancers. When the TCP peer matches one of these,
    /// rate limiting and the SIWE nonce cap key on the client IP from
    /// `X-Forwarded-For` / `X-Real-IP` instead of the proxy's address.
    ///
    /// Operator requirement: every proxy listed here MUST *append* the real
    /// peer to `X-Forwarded-For` (e.g. nginx `$proxy_add_x_forwarded_for`)
    /// and MUST NOT forward a client-supplied `X-Forwarded-For` verbatim.
    /// The rightmost-untrusted client lookup is spoof-resistant only because
    /// the hop the trusted proxy appends is the one entry the caller cannot
    /// control; a pass-through proxy lets the caller forge their own
    /// rate-limit / nonce key and evade or misattribute the per-IP budget.
    ///
    /// Empty (the default) means the listener is directly exposed and
    /// forwarding headers are ignored — set this whenever a TLS-terminating
    /// proxy or LB sits in front, or per-IP limits collapse onto the proxy
    /// IP and become a single shared budget (issue #159).
    #[arg(long, env = "DARKPOOL_TRUSTED_PROXIES", default_value = "")]
    pub trusted_proxies: String,

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

    /// Key URI for snapshot-at-rest encryption (#203). Same schemes as the
    /// operator key (`file:` / `age:` / `awskms:`), resolving to a 32-byte
    /// symmetric key. This is a *dedicated* key, separate from the operator
    /// ECIES identity, so its compromise cannot decrypt live orders and
    /// rotating the operator key never strands snapshot recovery.
    ///
    /// Required when snapshots use a durable store (file / postgres): the boot
    /// path refuses to start rather than write plaintext order data. The
    /// non-durable in-memory store uses a per-process ephemeral key when this
    /// is empty.
    #[arg(long, env = "DARKPOOL_SNAPSHOT_KEY_URI", default_value = "")]
    pub snapshot_key_uri: String,

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

    /// Versioned Groth16 verifying key for per-order commitment proofs.
    /// Production must set this to the `commitment_vk.bin` generated by
    /// `dp-zk-cli setup-commitment-circuit`; otherwise order proofs would be
    /// accepted without cryptographic verification.
    #[arg(long, env = "DARKPOOL_ORDER_PROOF_VK", default_value = "")]
    pub order_proof_vk: String,

    /// Local/dev escape hatch for running the server before a trusted
    /// commitment-circuit ceremony. Never enable in production.
    #[arg(
        long,
        env = "DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS",
        default_value = "false"
    )]
    pub allow_unverified_order_proofs: bool,

    /// Circuit batch size. Must equal the keygen-time value.
    #[arg(long, env = "DARKPOOL_ZK_BATCH_SIZE", default_value = "8")]
    pub zk_batch_size: u32,

    /// On-chain submission deadline. Falls back to aggregator_timeout when 0.
    #[arg(long, env = "DARKPOOL_SUBMIT_TIMEOUT", default_value = "0s", value_parser = parse_duration)]
    pub submit_timeout: Duration,

    #[arg(long, env = "DARKPOOL_ETH_RPC", default_value = "")]
    pub eth_rpc: String,

    /// Private transaction RPC used for signed settlement submissions.
    /// `DARKPOOL_ETH_RPC` remains the normal read RPC for chain state and the
    /// balance oracle; settlement writes carry plaintext cleared matches in
    /// calldata, so production operators must route them through a private tx
    /// endpoint (Flashbots Protect, MEV-Share, or a builder private RPC).
    #[arg(long, env = "DARKPOOL_SETTLEMENT_PRIVATE_RPC", default_value = "")]
    pub settlement_private_rpc: String,

    /// Local/dev escape hatch that lets signed settlement use
    /// `DARKPOOL_ETH_RPC` directly. Never enable for value-bearing
    /// deployments: public mempool observers can see the full cleared book
    /// before the settlement block lands.
    #[arg(
        long,
        env = "DARKPOOL_ALLOW_PUBLIC_SETTLEMENT",
        default_value = "false"
    )]
    pub allow_public_settlement: bool,

    #[arg(long, env = "DARKPOOL_CONTRACT_ADDR", default_value = "")]
    pub contract_addr: String,

    #[arg(long, env = "DARKPOOL_CHAIN_ID", default_value = "0")]
    pub chain_id: u64,

    #[arg(long, env = "DARKPOOL_SUBMIT_GAS", default_value = "500000")]
    pub submit_gas: u64,

    /// Enable SIWE (Sign-In with Ethereum) authentication. When enabled,
    /// traders can authenticate via wallet signature and receive a JWT.
    /// Static API keys remain as fallback for programmatic access.
    #[arg(long, env = "DARKPOOL_SIWE_ENABLED", default_value = "false")]
    pub siwe_enabled: bool,

    /// Secret used to sign/verify JWT session tokens. Required when
    /// SIWE is enabled; ignored otherwise.
    #[arg(long, env = "DARKPOOL_SESSION_SECRET", default_value = "")]
    session_secret_raw: String,

    /// JWT session token TTL. Defaults to 24 hours.
    #[arg(long, env = "DARKPOOL_SESSION_TTL", default_value = "24h", value_parser = parse_duration)]
    pub session_ttl: Duration,

    /// Expected SIWE message domain (RFC 3986 authority). When set,
    /// the server rejects SIWE messages whose `domain` field doesn't
    /// match — prevents cross-site replay attacks. Example:
    /// `app.darkpool.exchange` or `localhost:3000`.
    #[arg(long, env = "DARKPOOL_SIWE_DOMAIN", default_value = "")]
    siwe_domain_raw: String,
}

impl Config {
    pub fn api_keys(&self) -> Vec<String> {
        self.api_keys_raw
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }

    pub fn cors_origins(&self) -> Vec<String> {
        self.cors_origins_raw
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

    pub fn snapshot_key_uri_str(&self) -> Option<&str> {
        opt(&self.snapshot_key_uri)
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

    pub fn order_proof_vk_path(&self) -> Option<&str> {
        opt(&self.order_proof_vk)
    }

    pub fn eth_rpc_url(&self) -> Option<&str> {
        opt(&self.eth_rpc)
    }

    pub fn settlement_private_rpc_url(&self) -> Option<&str> {
        opt(&self.settlement_private_rpc)
    }

    pub fn contract_address(&self) -> Option<&str> {
        opt(&self.contract_addr)
    }

    pub fn session_secret(&self) -> Option<&str> {
        opt(&self.session_secret_raw)
    }

    pub fn siwe_domain(&self) -> Option<&str> {
        opt(&self.siwe_domain_raw)
    }

    pub fn validate_siwe_config(&self) -> Result<(), String> {
        if !self.siwe_enabled {
            return Ok(());
        }
        match self.session_secret() {
            None => {
                return Err(
                    "DARKPOOL_SIWE_ENABLED is true but DARKPOOL_SESSION_SECRET is not set".into(),
                );
            }
            Some(s) if s.len() < 32 => {
                return Err("DARKPOOL_SESSION_SECRET must be at least 32 bytes".into());
            }
            _ => {}
        }
        Ok(())
    }

    /// Fail-closed validation of the operator-admin authentication
    /// posture. An empty operator key set makes
    /// [`crate::auth::AuthCore::check`] authenticate every `/v1/admin/*`
    /// request — including ECIES key rotation. Reject it at boot unless
    /// the operator explicitly opts into unauthenticated admin with
    /// `--allow-unauthenticated-admin`.
    pub fn validate_admin_auth(&self) -> Result<(), String> {
        if self.operator_api_keys().is_empty() && !self.allow_unauthenticated_admin {
            return Err(
                "DARKPOOL_OPERATOR_API_KEYS is empty — admin endpoints (incl. ECIES key \
                 rotation) would accept unauthenticated requests. Set operator keys, or pass \
                 --allow-unauthenticated-admin / DARKPOOL_ALLOW_UNAUTHENTICATED_ADMIN=true for \
                 local dev."
                    .into(),
            );
        }
        Ok(())
    }

    /// Fail closed on limits that would make the server unusable or leave a
    /// requested guardrail disabled.
    pub fn validate_server_limits(&self) -> Result<(), String> {
        if self.request_timeout.is_zero() {
            return Err("DARKPOOL_REQUEST_TIMEOUT must be greater than zero".into());
        }
        if self.max_concurrent_requests == 0 {
            return Err("DARKPOOL_MAX_CONCURRENT_REQUESTS must be greater than zero".into());
        }
        if self.http2_max_concurrent_streams == 0 {
            return Err("DARKPOOL_HTTP2_MAX_CONCURRENT_STREAMS must be greater than zero".into());
        }
        if self.sse_streams_per_key == 0 {
            return Err("DARKPOOL_SSE_STREAMS_PER_KEY must be greater than zero".into());
        }
        Ok(())
    }

    /// Fail closed when signed settlement would submit cleartext match calldata
    /// to a public RPC. `DARKPOOL_ETH_RPC` is still required for chain reads and
    /// balance-oracle calls; signed writes must use a private transaction RPC
    /// unless the operator explicitly opts into public submission for local dev.
    pub fn validate_settlement_transport(&self) -> Result<(), String> {
        if self.settlement_private_rpc_url().is_some() && self.eth_rpc_url().is_none() {
            return Err(
                "DARKPOOL_SETTLEMENT_PRIVATE_RPC is set but DARKPOOL_ETH_RPC is missing. \
                 Set DARKPOOL_ETH_RPC for chain reads and balance-oracle calls."
                    .into(),
            );
        }

        let signed_settlement_enabled =
            self.eth_rpc_url().is_some() && self.signer_key_uri_str().is_some();
        if !signed_settlement_enabled {
            return Ok(());
        }

        if self.settlement_private_rpc_url().is_none() && !self.allow_public_settlement {
            return Err(
                "DARKPOOL_ETH_RPC and DARKPOOL_SIGNER_KEY_URI enable signed settlement, but \
                 DARKPOOL_SETTLEMENT_PRIVATE_RPC is not set. submitBatch/settleAuction calldata \
                 contains the full cleared book, so settlement must use a private transaction \
                 endpoint (Flashbots Protect, MEV-Share, or builder private RPC). For local \
                 dev only, set DARKPOOL_ALLOW_PUBLIC_SETTLEMENT=true."
                    .into(),
            );
        }

        Ok(())
    }

    /// Fail-closed guard against serving plaintext on an
    /// externally-reachable interface. With no TLS material configured
    /// the server defaults to binding `0.0.0.0` (every interface) in the
    /// clear — exposing the API key, the SIWE bearer token, and order
    /// ciphertext/metadata to any on-path observer. Refuse to boot in
    /// that posture unless every plaintext listener is loopback-only, or
    /// the operator explicitly accepts the risk with `--insecure`
    /// (e.g. a TLS-terminating proxy fronts the service). TLS / mTLS
    /// modes encrypt the wire and are always allowed.
    ///
    /// Takes the already-resolved [`TlsMode`] so the boot path validates
    /// the same posture it is about to bind, rather than re-deriving it.
    pub fn validate_plaintext_bind(&self, tls_mode: &TlsMode) -> Result<(), String> {
        if !matches!(tls_mode, TlsMode::Plaintext) || self.insecure {
            return Ok(());
        }
        let exposed: Vec<String> = [self.grpc_addr, self.http_addr]
            .iter()
            .filter(|addr| !addr.ip().is_loopback())
            .map(|addr| addr.to_string())
            .collect();
        if exposed.is_empty() {
            return Ok(());
        }
        Err(format!(
            "plaintext transport on non-loopback bind(s) {} — the API key, SIWE \
             bearer token, and order ciphertext/metadata would travel in the clear. \
             Configure TLS with --tls-cert/--tls-key (see \
             docs/operations/tls-setup.md), bind loopback only, or pass --insecure / \
             DARKPOOL_INSECURE=true to override (e.g. when a TLS-terminating proxy \
             fronts the service).",
            exposed.join(", "),
        ))
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
///   plaintext. Loopback-only is acceptable for local dev (main.rs logs
///   a loud warning); a non-loopback plaintext bind is a hard boot
///   failure unless `--insecure` is set. See
///   [`Config::validate_plaintext_bind`].
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

    #[test]
    fn snapshot_dir_path_empty_is_none() {
        let cfg = cfg_with_tls("", "", "");
        assert!(cfg.snapshot_dir_path().is_none());
    }

    #[test]
    fn snapshot_dir_path_set_returns_some() {
        let cfg = Config::parse_from(["darkpool-server", "--snapshot-dir", "/var/dp/snaps"]);
        assert_eq!(cfg.snapshot_dir_path(), Some("/var/dp/snaps"));
    }

    #[test]
    fn order_proof_vk_path_set_returns_some() {
        let cfg = Config::parse_from([
            "darkpool-server",
            "--order-proof-vk",
            "/var/dp/commitment_vk.bin",
        ]);
        assert_eq!(cfg.order_proof_vk_path(), Some("/var/dp/commitment_vk.bin"));
    }

    fn cfg_with_siwe(enabled: bool, secret: &str, domain: &str) -> Config {
        let mut args = vec!["darkpool-server".to_string()];
        if enabled {
            args.push("--siwe-enabled".into());
        }
        if !secret.is_empty() {
            args.push("--session-secret-raw".into());
            args.push(secret.into());
        }
        if !domain.is_empty() {
            args.push("--siwe-domain-raw".into());
            args.push(domain.into());
        }
        Config::parse_from(args)
    }

    #[test]
    fn siwe_disabled_validates_ok_without_secret() {
        let cfg = cfg_with_siwe(false, "", "");
        assert!(cfg.validate_siwe_config().is_ok());
    }

    #[test]
    fn siwe_enabled_without_secret_fails() {
        let cfg = cfg_with_siwe(true, "", "");
        let err = cfg.validate_siwe_config().unwrap_err();
        assert!(err.contains("SESSION_SECRET"), "msg: {err}");
    }

    #[test]
    fn siwe_enabled_with_short_secret_fails() {
        let cfg = cfg_with_siwe(true, "tooshort", "");
        let err = cfg.validate_siwe_config().unwrap_err();
        assert!(err.contains("32 bytes"), "msg: {err}");
    }

    #[test]
    fn siwe_enabled_with_valid_secret_passes() {
        let cfg = cfg_with_siwe(true, "a]secret-that-is-at-least-32-bytes-long!", "");
        assert!(cfg.validate_siwe_config().is_ok());
        assert!(cfg.session_secret().is_some());
    }

    #[test]
    fn siwe_domain_accessor() {
        let cfg = cfg_with_siwe(false, "", "app.darkpool.exchange");
        assert_eq!(cfg.siwe_domain(), Some("app.darkpool.exchange"));
        let cfg2 = cfg_with_siwe(false, "", "");
        assert!(cfg2.siwe_domain().is_none());
    }

    fn cfg_with_admin(operator_keys: &str, allow_unauth: bool) -> Config {
        // Pass the operator keys explicitly so the test is independent
        // of any ambient DARKPOOL_OPERATOR_API_KEYS in the environment.
        let mut args = vec![
            "darkpool-server".to_string(),
            "--operator-api-keys-raw".into(),
            operator_keys.into(),
        ];
        if allow_unauth {
            args.push("--allow-unauthenticated-admin".into());
        }
        Config::parse_from(args)
    }

    #[test]
    fn admin_auth_empty_keys_without_flag_fails() {
        let cfg = cfg_with_admin("", false);
        let err = cfg.validate_admin_auth().unwrap_err();
        assert!(err.contains("OPERATOR_API_KEYS"), "msg: {err}");
    }

    #[test]
    fn admin_auth_empty_keys_with_flag_ok() {
        let cfg = cfg_with_admin("", true);
        assert!(cfg.allow_unauthenticated_admin);
        assert!(cfg.validate_admin_auth().is_ok());
    }

    #[test]
    fn admin_auth_with_keys_ok_without_flag() {
        let cfg = cfg_with_admin("op-secret-1", false);
        assert!(!cfg.allow_unauthenticated_admin);
        assert!(cfg.validate_admin_auth().is_ok());
    }

    #[test]
    fn admin_auth_with_keys_ignores_flag() {
        let cfg = cfg_with_admin("op-secret-1", true);
        assert!(cfg.validate_admin_auth().is_ok());
        assert_eq!(cfg.operator_api_keys(), vec!["op-secret-1"]);
    }

    fn cfg_with_bind(grpc: &str, http: &str, insecure: bool) -> Config {
        let mut args = vec![
            "darkpool-server".to_string(),
            "--grpc-addr".into(),
            grpc.into(),
            "--http-addr".into(),
            http.into(),
        ];
        if insecure {
            args.push("--insecure".into());
        }
        Config::parse_from(args)
    }

    #[test]
    fn plaintext_bind_loopback_only_is_ok() {
        // Local dev on loopback needs no opt-in.
        let cfg = cfg_with_bind("127.0.0.1:9090", "127.0.0.1:8080", false);
        assert!(cfg.validate_plaintext_bind(&TlsMode::Plaintext).is_ok());
    }

    #[test]
    fn plaintext_bind_ipv6_loopback_is_ok() {
        let cfg = cfg_with_bind("[::1]:9090", "[::1]:8080", false);
        assert!(cfg.validate_plaintext_bind(&TlsMode::Plaintext).is_ok());
    }

    #[test]
    fn plaintext_bind_non_loopback_fails_without_insecure() {
        // 0.0.0.0 (the default) is unspecified, not loopback — externally
        // reachable, so plaintext there must be refused at boot.
        let cfg = cfg_with_bind("0.0.0.0:9090", "0.0.0.0:8080", false);
        let err = cfg
            .validate_plaintext_bind(&TlsMode::Plaintext)
            .unwrap_err();
        assert!(err.contains("0.0.0.0:9090"), "msg: {err}");
        assert!(err.contains("0.0.0.0:8080"), "msg: {err}");
        assert!(err.contains("--insecure"), "msg: {err}");
    }

    #[test]
    fn plaintext_bind_non_loopback_ok_with_insecure() {
        let cfg = cfg_with_bind("0.0.0.0:9090", "0.0.0.0:8080", true);
        assert!(cfg.insecure);
        assert!(cfg.validate_plaintext_bind(&TlsMode::Plaintext).is_ok());
    }

    #[test]
    fn plaintext_bind_flags_only_the_exposed_listener() {
        // gRPC on loopback, REST on every interface → only REST is flagged.
        let cfg = cfg_with_bind("127.0.0.1:9090", "0.0.0.0:8080", false);
        let err = cfg
            .validate_plaintext_bind(&TlsMode::Plaintext)
            .unwrap_err();
        assert!(err.contains("0.0.0.0:8080"), "msg: {err}");
        assert!(
            !err.contains("9090"),
            "loopback listener must not be flagged: {err}"
        );
    }

    #[test]
    fn tls_bind_non_loopback_ok_without_insecure() {
        // TLS encrypts the wire — a non-loopback bind needs no override.
        let cfg = cfg_with_bind("0.0.0.0:9090", "0.0.0.0:8080", false);
        let mode = TlsMode::Tls {
            cert: "/srv/cert.pem".into(),
            key: "/srv/key.pem".into(),
        };
        assert!(cfg.validate_plaintext_bind(&mode).is_ok());
    }

    #[test]
    fn mtls_bind_non_loopback_ok_without_insecure() {
        let cfg = cfg_with_bind("0.0.0.0:9090", "0.0.0.0:8080", false);
        let mode = TlsMode::Mtls {
            cert: "/srv/cert.pem".into(),
            key: "/srv/key.pem".into(),
            client_ca: "/srv/ca.pem".into(),
        };
        assert!(cfg.validate_plaintext_bind(&mode).is_ok());
    }
}
