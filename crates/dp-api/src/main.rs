use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use dp_aggregator::SubprocessAggregator;
use dp_api::admin::{AdminApiHandler, KeyAdminHandler};
use dp_api::auth::{AuthCore, AuthLayer};
use dp_api::config::{Config, TlsMode};
use dp_api::handler::ApiHandler;
use dp_api::observability::{self, M_EVENT_LOG_SIZE_BYTES};
use dp_api::pb::dark_pool_service_server::DarkPoolServiceServer;
use dp_api::ratelimit::{RateLimitCore, RateLimitLayer, TrustedProxies};
use dp_api::readiness::{aggregator_probe, store_probe, ReadinessProbes};
use dp_api::rest::{self, OpsState};
use dp_api::tls;
use dp_api::validation::PLACE_ORDER_BODY_LIMIT;
use dp_crypto::{
    decrypter_from_uri, validate_key_id, EciesDecrypter, KeyEntry, KeyStatus, MultiKeyDecrypter,
    SnapshotCipher,
};
use dp_engine::{Engine, Groth16OrderProofVerifier, PairConfig, PairStatus, SnapshotConfig};
use dp_event::{
    FileSnapshotStore, FileStore, MemSnapshotStore, MemStore, PgSnapshotStore, PgStore,
    SnapshotStore, Store,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::timeout::{RequestBodyTimeoutLayer, TimeoutLayer};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Metrics first: the recorder must be installed before any
    // `metrics::counter!` call so the Prometheus exporter sees them.
    let prom_handle = observability::init_metrics()?;
    let tracing_guard = observability::init_tracing()?;

    let cfg = Config::parse();

    // Validate the auth posture before any side effects (store
    // connections, state recovery, pair seeding, task spawns) so an
    // invalid auth config fails closed without ever writing or ticking.
    cfg.validate_siwe_config()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    cfg.validate_admin_auth()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    cfg.validate_server_limits()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    cfg.validate_settlement_transport()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

    // Resolve the TLS posture up-front: half-configured TLS (cert
    // without key, etc.) must surface at boot, not when the first
    // client connects. The plaintext branch logs a loud warning so a
    // misconfigured prod deploy is never silent — there is no
    // "default-on" TLS today.
    let tls_mode = cfg
        .tls_mode()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    // Fail closed before binding: plaintext on a non-loopback interface
    // leaks credentials and order metadata on the wire. Loopback-only
    // plaintext (local dev) and an explicit --insecure override are the
    // only ways past this check.
    cfg.validate_plaintext_bind(&tls_mode)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    if matches!(tls_mode, TlsMode::Plaintext) {
        warn!(
            grpc = %cfg.grpc_addr,
            rest = %cfg.http_addr,
            "TLS DISABLED — server is binding plaintext on both listeners. \
             Set --tls-cert/--tls-key (and optionally --tls-client-ca for mTLS) \
             before exposing the server to a network you do not control."
        );
    }

    let store: Arc<dyn Store> = if let Some(url) = cfg.event_db_url() {
        info!(url = %sanitize_db_url(url), "event log: postgres");
        Arc::new(PgStore::connect(url).await?)
    } else if let Some(path) = cfg.event_log_path() {
        info!(path = %path, "event log: file");
        Arc::new(FileStore::open(path)?)
    } else {
        info!("event log: in-memory (not durable)");
        Arc::new(MemStore::new())
    };

    // Snapshot backend selection mirrors the event-log backend. The
    // postgres store keeps snapshots in the same database so a single
    // backup-and-restore captures both; file mode writes envelopes to
    // a dedicated directory; in-memory mode falls back to an in-memory
    // snapshot store (useful for tests, no durability).
    let snapshot_store: Option<Arc<dyn SnapshotStore>> = if !cfg.snapshot_enabled {
        info!("snapshots disabled — recover() will always do full event replay");
        None
    } else if let Some(url) = cfg.event_db_url() {
        info!(url = %sanitize_db_url(url), "snapshot store: postgres");
        Some(Arc::new(PgSnapshotStore::connect(url).await?))
    } else if let Some(dir) = cfg.snapshot_dir_path() {
        info!(dir = %dir, "snapshot store: file");
        Some(Arc::new(FileSnapshotStore::open(dir)?))
    } else if cfg.event_log_path().is_some() {
        warn!(
            "DARKPOOL_EVENT_LOG is set but DARKPOOL_SNAPSHOT_DIR is empty — \
             snapshots disabled. Set --snapshot-dir for periodic checkpoints."
        );
        None
    } else {
        info!("snapshot store: in-memory (not durable)");
        Some(Arc::new(MemSnapshotStore::new()))
    };

    let engine = Engine::new(store.clone(), cfg.auction_interval);
    engine.set_snapshot_store(snapshot_store.clone());

    if let Some(path) = cfg.order_proof_vk_path() {
        let verifier = Groth16OrderProofVerifier::from_file(path)?;
        engine.set_order_proof_verifier(Arc::new(verifier));
        info!(path = %path, "order proof verifier: canonical VK loaded");
    } else if cfg.allow_unverified_order_proofs {
        warn!(
            "DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true — per-order proofs will not be \
             cryptographically verified. Local/dev only; do not use in production."
        );
    } else {
        return Err(
            "DARKPOOL_ORDER_PROOF_VK must point to commitment_vk.bin. To run an unsafe local \
             fixture without per-order proof verification, set \
             DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true."
                .into(),
        );
    }

    // Snapshot-at-rest encryption (#203). A snapshot store without a cipher
    // would persist cleartext order data (trader / price / size), so fail
    // closed for durable backends and use a process-lifetime key for the
    // non-durable in-memory store.
    if snapshot_store.is_some() {
        let cipher = if let Some(uri) = cfg.snapshot_key_uri_str() {
            let c = SnapshotCipher::from_key_uri(uri)?;
            info!(uri = %sanitize_uri_for_log(uri), "snapshot cipher: key loaded");
            c
        } else if cfg.event_db_url().is_some() || cfg.snapshot_dir_path().is_some() {
            return Err(
                "snapshots are enabled with a durable store but DARKPOOL_SNAPSHOT_KEY_URI \
                 is unset — refusing to write plaintext order data at rest. Set a snapshot \
                 key (e.g. file:/path/to/key.hex) or disable snapshots \
                 (DARKPOOL_SNAPSHOT_ENABLED=false)."
                    .into(),
            );
        } else {
            warn!(
                "snapshot store is in-memory and DARKPOOL_SNAPSHOT_KEY_URI is unset — using \
                 an ephemeral per-process snapshot key. In-memory snapshots are not durable \
                 across restarts; configure a durable store + key URI for real recovery."
            );
            SnapshotCipher::generate_ephemeral()
        };
        engine.set_snapshot_cipher(Some(Arc::new(cipher)));
    }

    // Always construct a MultiKeyDecrypter and wire it to the engine
    // so admin-time rotation does not require restarting the process.
    // When no keys are configured the engine sees an empty decrypter
    // and `place_order` rejects ciphertexts with a clear "no
    // decryption keys registered" diagnostic — the same shape as a
    // misconfiguration today.
    let multi = MultiKeyDecrypter::new();
    let mut multi_seeded = false;
    if let Some(uris) = cfg.operator_key_uris_str() {
        for raw in uris.split(',') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (uri, status, id) = parse_key_uri_spec(trimmed, multi_seeded)?;
            // Operator-supplied `#id` suffixes must conform to the
            // same shape the admin endpoint enforces — fail at boot
            // rather than after a metric series is already poisoned.
            validate_key_id(&id)?;
            let decrypter = decrypter_from_uri(&uri)?;
            multi.insert(KeyEntry::new(id.clone(), status, decrypter));
            multi_seeded = true;
            info!(
                key_id = %id,
                uri = %sanitize_uri_for_log(&uri),
                status = %status,
                "decrypter: ECIES key registered"
            );
        }
    } else if let Some(key_path) = cfg.operator_key_path() {
        // Fail-fast: surface bad operator-key paths at boot rather
        // than on first decrypt attempt. Single-key legacy mode keeps
        // working without touching DARKPOOL_OPERATOR_KEY_URIS.
        let dec = EciesDecrypter::from_file(key_path)?;
        multi.insert(KeyEntry::new("primary", KeyStatus::Active, Arc::new(dec)));
        multi_seeded = true;
        info!(key = %key_path, "decrypter: ECIES (single-key)");
    }
    if multi_seeded {
        engine.set_decrypter(Arc::new(multi.clone()));
    } else {
        info!(
            "decrypter: noop (set --operator-key or --operator-key-uris to enable). \
             Admin endpoint POST /v1/admin/keys can register keys at runtime."
        );
        // Still install the multi so the admin endpoint can register
        // keys live without a restart; an empty multi rejects orders
        // until at least one key is added.
        engine.set_decrypter(Arc::new(multi.clone()));
    }

    if let Some(agg_bin) = cfg.aggregator_bin_path() {
        let agg_path = std::path::Path::new(agg_bin);
        // Fail-fast: aggregator binary must exist and be a regular file at
        // boot; otherwise every auction tick will fail under the noisy
        // subprocess-spawn error path.
        if !agg_path.is_file() {
            return Err(format!(
                "aggregator binary not found at {}: set --aggregator-bin to a valid path",
                agg_bin
            )
            .into());
        }
        // Fail-fast: when the ZK aggregator is wired, the proving-key dir
        // must exist and contain proving_key.bin / verifying_key.bin /
        // keys_metadata.json. Bad/missing keys would otherwise blow up on
        // the first batch with cryptic subprocess stderr.
        if let Some(key_dir) = cfg.zk_proving_key_dir() {
            validate_zk_key_dir(key_dir)?;
        } else {
            return Err(
                "DARKPOOL_ZK_PROVING_KEY must be set when DARKPOOL_AGGREGATOR_BIN is set".into(),
            );
        }
        let timeout = if cfg.aggregator_timeout.is_zero() {
            None
        } else {
            Some(cfg.aggregator_timeout)
        };
        // Forward ZK config to the spawned subprocess via per-spawn
        // Command::env. std::env::set_var is UB in a multi-threaded
        // process (Rust ≥ 1.74) and tokio's runtime is multi-threaded.
        let mut agg = SubprocessAggregator::new(agg_path, timeout)?;
        agg = agg.with_env(
            "DARKPOOL_ZK_PROVING_KEY",
            cfg.zk_proving_key_dir().unwrap_or_default(),
        );
        agg = agg.with_env("DARKPOOL_ZK_BATCH_SIZE", cfg.zk_batch_size.to_string());
        engine.set_aggregator(Arc::new(agg));
        let submit_timeout = if cfg.submit_timeout.is_zero() {
            cfg.aggregator_timeout
        } else {
            cfg.submit_timeout
        };
        engine.set_submit_timeout(submit_timeout);
        info!(bin = %agg_bin, "aggregator: subprocess");
    } else {
        info!("aggregator: noop (set --aggregator-bin to enable)");
    }

    if let Some(rpc) = cfg.eth_rpc_url() {
        if let Some(uri) = cfg.signer_key_uri_str() {
            let signer = dp_settlement::signer::from_uri(uri)?;
            let contract_addr = cfg.contract_address().ok_or(
                "DARKPOOL_ETH_RPC and DARKPOOL_SIGNER_KEY_URI are set but \
                 DARKPOOL_CONTRACT_ADDR is missing — cannot build EthSubmitter",
            )?;
            let (settlement_rpc, tx_transport) =
                if let Some(private_rpc) = cfg.settlement_private_rpc_url() {
                    (
                        private_rpc,
                        dp_settlement::SettlementTxTransport::PrivateRpc,
                    )
                } else {
                    warn!(
                        "DARKPOOL_ALLOW_PUBLIC_SETTLEMENT=true — settlement calldata will be \
                         sent through DARKPOOL_ETH_RPC and may expose the full cleared book in \
                         the public mempool. Local/dev only."
                    );
                    (rpc, dp_settlement::SettlementTxTransport::PublicMempool)
                };

            let read_provider = alloy_provider::ProviderBuilder::new().connect_http(
                rpc.parse()
                    .map_err(|e| format!("bad DARKPOOL_ETH_RPC URL: {e}"))?,
            );
            let submit_provider = alloy_provider::ProviderBuilder::new()
                .wallet(signer.wallet())
                .connect_http(settlement_rpc.parse().map_err(|e| {
                    if tx_transport == dp_settlement::SettlementTxTransport::PrivateRpc {
                        format!("bad DARKPOOL_SETTLEMENT_PRIVATE_RPC URL: {e}")
                    } else {
                        format!("bad DARKPOOL_ETH_RPC URL: {e}")
                    }
                })?);

            let submitter_cfg = dp_settlement::EthSubmitterConfig {
                signer,
                contract_address: contract_addr.to_string(),
                chain_id: cfg.chain_id,
                gas_limit: Some(cfg.submit_gas),
                tx_transport,
            };

            let submitter = dp_settlement::EthSubmitter::with_submit_provider(
                read_provider,
                submit_provider,
                &submitter_cfg,
            )?;
            engine.set_submitter(Arc::new(submitter));

            info!(
                read_rpc = %rpc,
                contract = %contract_addr,
                chain_id = cfg.chain_id,
                tx_transport = tx_transport.as_str(),
                "batch submitter: on-chain (EthSubmitter)"
            );

            // Value-bearing deployment: fail closed on the InsecureDevOracle
            // default. The matching circuit's solvency witness must come from
            // real on-chain `reserved` collateral, not a fabricated 1B balance
            // (#213). A read-only provider (no signer needed for eth_call) backs
            // the oracle; a bad address/URL aborts boot rather than serving
            // traffic whose solvency proof attests nothing about real funds.
            let oracle_contract = contract_addr
                .parse::<alloy_primitives::Address>()
                .map_err(|e| format!("bad DARKPOOL_CONTRACT_ADDR: {e}"))?;
            let oracle_provider = alloy_provider::ProviderBuilder::new().connect_http(
                rpc.parse()
                    .map_err(|e| format!("bad DARKPOOL_ETH_RPC URL: {e}"))?,
            );
            engine.set_balance_oracle(Arc::new(dp_settlement::ChainBalanceOracle::new(
                oracle_provider,
                oracle_contract,
            )));
            info!(
                contract = %contract_addr,
                "balance oracle: on-chain (reads reserved[trader][asset])"
            );
        } else {
            warn!(
                rpc = %rpc,
                "DARKPOOL_ETH_RPC is set but DARKPOOL_SIGNER_KEY_URI is not; \
                 running with noop submitter — auctions will NOT be settled on-chain. \
                 Set --signer-key-uri to point at a TxSigner backend."
            );
        }
    } else {
        info!("batch submitter: noop (set --eth-rpc to enable on-chain settlement)");
    }

    engine.recover().await?;

    // Seed the pair registry on first boot when no pairs were replayed.
    // After this point new pairs come exclusively via the admin API.
    if engine.list_pairs().is_empty() {
        if let Some(seed) = cfg.pair_seed_json_str() {
            seed_pairs_from_json(&engine, seed)?;
        } else {
            warn!(
                "pair registry empty and no DARKPOOL_PAIR_SEED_JSON provided; \
                 all PlaceOrder requests will fail until an operator registers a pair"
            );
        }
    }

    let cancel = CancellationToken::new();

    let engine_tick = engine.clone();
    let tick_cancel = cancel.clone();
    let engine_handle = tokio::spawn(async move {
        engine_tick.start(tick_cancel).await;
    });

    // Spawn the periodic state snapshotter when a SnapshotStore is wired.
    // The task owns its own `tokio::time::interval`; cancellation flows
    // through the shared `CancellationToken` so shutdown is in lockstep
    // with the auction tick and REST/gRPC servers.
    // `snapshot_store` is None whenever `cfg.snapshot_enabled` is false
    // (see the construction block above), so `is_some()` is the only
    // check needed here.
    let snapshot_handle = if snapshot_store.is_some() {
        let snapshot_cfg = SnapshotConfig {
            enabled: true,
            every_events: cfg.snapshot_every_events,
            interval: cfg.snapshot_interval,
            retain_events: cfg.snapshot_retain_events,
            retain_count: cfg.snapshot_retain_count,
        };
        let engine_snap = engine.clone();
        let snap_cancel = cancel.clone();
        Some(tokio::spawn(async move {
            engine_snap.run_snapshotter(snapshot_cfg, snap_cancel).await;
        }))
    } else {
        None
    };

    let siwe_state = if cfg.siwe_enabled {
        let secret = cfg.session_secret().unwrap();
        let jwt_manager = Arc::new(dp_api::siwe::JwtManager::new(secret, cfg.session_ttl));
        let nonce_store = Arc::new(dp_api::siwe::NonceStore::new(dp_api::siwe::NONCE_TTL));
        nonce_store.start_cleanup(cancel.clone(), Duration::from_secs(60));
        let chain_id = if cfg.chain_id > 0 {
            Some(cfg.chain_id)
        } else {
            None
        };
        info!(
            chain_id = ?chain_id,
            session_ttl = ?cfg.session_ttl,
            "SIWE authentication enabled"
        );
        let expected_domain = cfg.siwe_domain().map(|s| s.to_string());
        Some(dp_api::siwe::SiweState {
            nonce_store,
            jwt_manager: jwt_manager.clone(),
            chain_id,
            expected_domain,
        })
    } else {
        info!("SIWE authentication disabled (set --siwe-enabled to activate)");
        None
    };

    let auth_core = if let Some(ref siwe) = siwe_state {
        AuthCore::new_with_jwt(cfg.api_keys(), siwe.jwt_manager.clone())
    } else {
        AuthCore::new(cfg.api_keys())
    };
    let operator_keys = cfg.operator_api_keys();
    if operator_keys.is_empty() {
        // Reachable only when --allow-unauthenticated-admin opted in;
        // validate_admin_auth() aborts boot otherwise.
        warn!(
            "DARKPOOL_OPERATOR_API_KEYS is empty and --allow-unauthenticated-admin is set \
             — admin endpoints accept UNAUTHENTICATED requests, including ECIES key \
             rotation. Never use this outside local dev."
        );
    }
    let admin_auth_core = AuthCore::new(operator_keys);
    let trusted_proxies = TrustedProxies::parse(&cfg.trusted_proxies)
        .map_err(|e| format!("DARKPOOL_TRUSTED_PROXIES: {e}"))?;
    if trusted_proxies.is_empty() {
        info!(
            "no trusted proxies configured; rate limiting keys on the TCP peer IP. \
             Set DARKPOOL_TRUSTED_PROXIES if a reverse proxy or load balancer fronts \
             this listener (otherwise per-IP limits collapse onto the proxy IP)."
        );
    } else {
        info!(
            trusted_proxies = %cfg.trusted_proxies,
            "trusting X-Forwarded-For / X-Real-IP from configured proxy ranges"
        );
    }
    let rl_core = RateLimitCore::with_trusted_proxies(
        cfg.rate_limit,
        cfg.rate_burst,
        cfg.rate_stale_after,
        trusted_proxies,
    );
    rl_core.start_cleanup(cancel.clone(), Duration::from_secs(60));

    let request_timeout = cfg.request_timeout;
    let http2_max_concurrent_streams = cfg.http2_max_concurrent_streams;
    let global_concurrency = Arc::new(Semaphore::new(cfg.max_concurrent_requests));
    info!(
        request_timeout = ?request_timeout,
        max_concurrent_requests = cfg.max_concurrent_requests,
        http2_max_concurrent_streams,
        sse_streams_per_key = cfg.sse_streams_per_key,
        "server request guardrails enabled"
    );

    let auth = AuthLayer::from_core(auth_core.clone());
    let ratelimit = RateLimitLayer::from_core(rl_core.clone());

    let handler = ApiHandler::new(engine.clone()).with_auction_stream_timeout(request_timeout);
    let admin_handler = AdminApiHandler::new(engine.clone());
    let key_admin_handler = KeyAdminHandler::new(multi.clone());
    let shared = Arc::new(handler.clone());
    let shared_admin = Arc::new(admin_handler.clone());
    let shared_key_admin = Arc::new(key_admin_handler.clone());

    let grpc_cancel = cancel.clone();
    let grpc_addr = cfg.grpc_addr;
    let grpc_auth = auth.clone();
    let grpc_rl = ratelimit.clone();
    let grpc_concurrency = GlobalConcurrencyLimitLayer::with_semaphore(global_concurrency.clone());
    let grpc_request_timeout = request_timeout;
    let grpc_http2_max_concurrent_streams = http2_max_concurrent_streams;
    // The admin service is intentionally NOT mounted on the gRPC port:
    // the gRPC listener authenticates with the trader API key set, which
    // would let any trader call RegisterPair/SuspendPair/DelistPair if the
    // admin service were attached here. Admin RPCs ride on the REST
    // `/v1/admin/*` paths only (gated by the separate operator key set in
    // [`rest::router_with_admin`]). Restoring gRPC admin will require a
    // dedicated listener on its own port with the operator AuthLayer.
    let grpc_tls = tls::tonic_server_tls(&tls_mode)?;
    let grpc_tls_enabled = grpc_tls.is_some();
    let grpc_handle = tokio::spawn(async move {
        info!(addr = %grpc_addr, tls = grpc_tls_enabled, "gRPC server starting");
        let mut builder = Server::builder()
            .timeout(grpc_request_timeout)
            .max_concurrent_streams(Some(grpc_http2_max_concurrent_streams))
            .layer(grpc_concurrency);
        if let Some(tls) = grpc_tls {
            builder = builder
                .tls_config(tls)
                .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
        }
        builder
            .layer(grpc_auth)
            .layer(grpc_rl)
            .add_service(
                DarkPoolServiceServer::new(handler)
                    .max_decoding_message_size(PLACE_ORDER_BODY_LIMIT),
            )
            .serve_with_shutdown(grpc_addr, async move { grpc_cancel.cancelled().await })
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))
    });

    let readiness = ReadinessProbes::new()
        .with(store_probe(store.clone()))
        .with(aggregator_probe(
            cfg.aggregator_bin_path().map(|s| s.to_string()),
        ));
    let ops = OpsState {
        prom: prom_handle,
        readiness,
        multi: multi.clone(),
    };

    // Periodic gauge: event-log size. Polled off the request path so a
    // slow PgStore round-trip never blocks a scrape; 30 s matches the
    // typical Prometheus interval without hammering postgres.
    //
    // `event_log_size_bytes()` is a synchronous `Store` method; PgStore
    // bridges to async via `block_in_place`, which parks one tokio worker
    // thread per call. Every 30 s is low-frequency enough that one parked
    // worker per tick is tolerable; do not lower the interval without
    // switching the call to a dedicated blocking task.
    let gauge_engine = engine.clone();
    let gauge_cancel = cancel.clone();
    let gauge_handle = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = gauge_cancel.cancelled() => return,
                _ = tick.tick() => {
                    match gauge_engine.event_log_size_bytes() {
                        Ok(bytes) => metrics::gauge!(M_EVENT_LOG_SIZE_BYTES).set(bytes as f64),
                        // warn (not debug) so a persistent PgStore outage
                        // doesn't leave the gauge silently frozen — without
                        // a log line, operators have no signal that the
                        // last-known value is stale.
                        Err(e) => tracing::warn!("event_log_size_bytes: {e}"),
                    }
                }
            }
        }
    });

    let cors_origins = cfg.cors_origins();
    let rest_app = rest::router_with_ops(
        shared,
        shared_admin,
        shared_key_admin,
        auth_core,
        admin_auth_core,
        rl_core,
        ops,
        &cors_origins,
        siwe_state,
        cfg.sse_streams_per_key,
    )
    .layer(RequestBodyTimeoutLayer::new(request_timeout))
    .layer(TimeoutLayer::new(request_timeout))
    .layer(GlobalConcurrencyLimitLayer::with_semaphore(
        global_concurrency.clone(),
    ));
    let http_addr = cfg.http_addr;
    let http_cancel = cancel.clone();
    let rest_tls = tls::axum_rustls_config(&tls_mode).await?;
    // Keep a clone alive in main for the SIGHUP reload task — moving
    // the option into the spawn would orphan the handle from the
    // reload path. `RustlsConfig` is `Arc<...>` internally so the
    // clone is cheap.
    let rest_tls_for_reload = rest_tls.clone();
    let rest_tls_enabled = rest_tls.is_some();
    let rest_handle = tokio::spawn(async move {
        info!(addr = %http_addr, tls = rest_tls_enabled, "REST server starting");
        let make_svc = rest_app.into_make_service_with_connect_info::<SocketAddr>();
        let server_handle = axum_server::Handle::new();
        let shutdown_handle = server_handle.clone();
        tokio::spawn(async move {
            http_cancel.cancelled().await;
            // Bound the in-flight HTTPS drain at 10s — beyond that
            // axum_server drops live connections rather than holding
            // shutdown indefinitely.
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(10)));
        });
        let result = match rest_tls {
            Some(cfg) => {
                let mut server = axum_server::bind_rustls(http_addr, cfg);
                server
                    .http_builder()
                    .http1()
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(Some(request_timeout));
                server
                    .http_builder()
                    .http2()
                    .max_concurrent_streams(Some(http2_max_concurrent_streams));
                server.handle(server_handle).serve(make_svc).await
            }
            None => {
                let mut server = axum_server::bind(http_addr);
                server
                    .http_builder()
                    .http1()
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(Some(request_timeout));
                server
                    .http_builder()
                    .http2()
                    .max_concurrent_streams(Some(http2_max_concurrent_streams));
                server.handle(server_handle).serve(make_svc).await
            }
        };
        result.map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))
    });

    // SIGHUP-driven TLS hot-reload for the REST listener. The gRPC
    // listener has no in-place reload path in tonic 0.12, so SIGHUP
    // emits a warn pointing operators at the restart-only flow for the
    // gRPC cert. Documented asymmetry — see docs/operations/tls-setup.md.
    #[cfg(unix)]
    let reload_handle = {
        let reload_cancel = cancel.clone();
        let reload_rest_cfg = rest_tls_for_reload.clone();
        let cfg_for_reload = cfg.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut hup = match signal(SignalKind::hangup()) {
                Ok(s) => s,
                Err(e) => {
                    warn!("failed to install SIGHUP handler — TLS hot-reload disabled: {e}");
                    return;
                }
            };
            loop {
                tokio::select! {
                    _ = reload_cancel.cancelled() => return,
                    sig = hup.recv() => {
                        if sig.is_none() {
                            return;
                        }
                        let mode = match cfg_for_reload.tls_mode() {
                            Ok(m) => m,
                            Err(e) => {
                                warn!("SIGHUP: bad TLS config, ignoring reload: {e}");
                                continue;
                            }
                        };
                        if matches!(mode, TlsMode::Plaintext) {
                            warn!("SIGHUP received but TLS is disabled — nothing to reload");
                            continue;
                        }
                        if let Some(rest_cfg) = &reload_rest_cfg {
                            match tls::reload_axum_rustls(rest_cfg, &mode).await {
                                Ok(()) => info!("SIGHUP: REST TLS material reloaded"),
                                Err(e) => warn!(
                                    "SIGHUP: REST TLS reload failed (keeping previous cert): {e}"
                                ),
                            }
                        }
                        warn!(
                            "SIGHUP: gRPC listener does not hot-reload TLS — restart the \
                             process to rotate the gRPC certificate"
                        );
                    }
                }
            }
        })
    };
    // Silence the unused-variable warning when the SIGHUP task is not
    // compiled in (non-unix builds — no signal API to listen on).
    #[cfg(not(unix))]
    let _ = rest_tls_for_reload;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => info!("ctrl+c received"),
        _ = sigterm() => info!("SIGTERM received"),
    }

    info!("shutting down...");
    cancel.cancel();

    let _ = engine_handle.await;
    let _ = grpc_handle.await;
    let _ = rest_handle.await;
    let _ = gauge_handle.await;
    if let Some(h) = snapshot_handle {
        // Surface task panics — silently dropping the JoinError would
        // hide a snapshotter crash from the operator.
        if let Err(e) = h.await {
            if e.is_panic() {
                tracing::error!(error = ?e, "snapshotter task panicked");
            } else {
                tracing::warn!(error = ?e, "snapshotter task join error");
            }
        }
    }
    #[cfg(unix)]
    let _ = reload_handle.await;

    // Flush any in-flight OTLP batches before the process exits.
    tracing_guard.shutdown();

    info!("shutdown complete");
    Ok(())
}

#[cfg(unix)]
async fn sigterm() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut s = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    s.recv().await;
}

#[cfg(not(unix))]
async fn sigterm() {
    std::future::pending::<()>().await;
}

/// Validate that the ZK proving-key directory contains the artifacts the
/// aggregator subprocess will load on first batch. We check existence only
/// (not byte validity) so boot stays cheap; mismatched key contents still
/// fail later, but missing files surface immediately.
fn validate_zk_key_dir(dir: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let p = std::path::Path::new(dir);
    if !p.is_dir() {
        return Err(format!("ZK proving-key dir not found: {}", dir).into());
    }
    for f in ["proving_key.bin", "verifying_key.bin", "keys_metadata.json"] {
        let fp = p.join(f);
        if !fp.is_file() {
            return Err(format!("ZK proving-key dir missing {}: {}", f, dir).into());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct PairSeedEntry {
    pair: String,
    #[serde(alias = "baseToken")]
    base_token: String,
    #[serde(alias = "quoteToken")]
    quote_token: String,
    #[serde(default, alias = "minOrderSize")]
    min_order_size: String,
    #[serde(default, alias = "tickSize")]
    tick_size: String,
    #[serde(default, alias = "auctionIntervalMs")]
    auction_interval_ms: Option<u64>,
    /// On-chain ERC20 decimals of each token (#211). Omit to default to 18.
    #[serde(default, alias = "baseDecimals")]
    base_decimals: Option<u8>,
    #[serde(default, alias = "quoteDecimals")]
    quote_decimals: Option<u8>,
}

/// Validate an optional seed `decimals` field, mirroring the `<= 30` bound the
/// admin `register_pair` RPC enforces, so a JSON seed cannot persist a pair the
/// operator API would itself refuse to register. Absent defaults to 18.
fn seed_decimals(
    v: Option<u8>,
    field: &str,
    pair: &str,
) -> Result<u8, Box<dyn std::error::Error + Send + Sync>> {
    match v {
        None => Ok(18),
        Some(d) if d <= 30 => Ok(d),
        Some(d) => Err(format!("pair seed {pair}: {field} must be <= 30, got {d}").into()),
    }
}

/// Apply the `DARKPOOL_PAIR_SEED_JSON` seed. Each entry becomes a
/// `PairRegistered` event so the seed survives subsequent restarts via
/// replay (the seed env var is consulted only on first boot).
fn seed_pairs_from_json(
    engine: &Engine,
    seed: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entries: Vec<PairSeedEntry> = serde_json::from_str(seed)
        .map_err(|e| format!("DARKPOOL_PAIR_SEED_JSON parse error: {e}"))?;
    if entries.is_empty() {
        return Ok(());
    }
    for e in entries {
        let base = alloy_primitives::Address::from_str(e.base_token.trim())
            .map_err(|err| format!("pair seed {}: bad base_token: {err}", e.pair))?;
        let quote = alloy_primitives::Address::from_str(e.quote_token.trim())
            .map_err(|err| format!("pair seed {}: bad quote_token: {err}", e.pair))?;
        let min = if e.min_order_size.is_empty() {
            Decimal::ZERO
        } else {
            Decimal::from_str(e.min_order_size.trim())
                .map_err(|err| format!("pair seed {}: bad min_order_size: {err}", e.pair))?
        };
        let tick = if e.tick_size.is_empty() {
            Decimal::ZERO
        } else {
            Decimal::from_str(e.tick_size.trim())
                .map_err(|err| format!("pair seed {}: bad tick_size: {err}", e.pair))?
        };
        let base_decimals = seed_decimals(e.base_decimals, "base_decimals", &e.pair)?;
        let quote_decimals = seed_decimals(e.quote_decimals, "quote_decimals", &e.pair)?;
        let cfg = PairConfig {
            base_token: base,
            quote_token: quote,
            base_decimals,
            quote_decimals,
            min_order_size: min,
            tick_size: tick,
            auction_interval: e.auction_interval_ms.map(Duration::from_millis),
            status: PairStatus::Active,
        };
        let canonical = engine
            .register_pair_with_event(&e.pair, cfg)
            .map_err(|err| format!("pair seed {}: {err}", e.pair))?;
        info!(pair = %canonical, "seeded pair");
    }
    Ok(())
}

/// Parse a single entry of `DARKPOOL_OPERATOR_KEY_URIS`. The wire
/// format is `<uri>[#<id>][@<status>]`:
/// - `uri` — required, passed verbatim to `decrypter_from_uri`.
/// - `id` — optional operator-chosen handle. Defaults to a derived
///   label (`key-<n>`) so log lines and metrics always carry a
///   non-empty `key_id`.
/// - `status` — optional; one of `active|rotating|sunset`. Defaults
///   to `active` for the first entry of the list and `rotating` for
///   every subsequent entry so a careless list never silently demotes
///   the primary.
fn parse_key_uri_spec(
    spec: &str,
    already_seeded: bool,
) -> Result<(String, KeyStatus, String), Box<dyn std::error::Error + Send + Sync>> {
    // Split off `@status` from the right so URIs containing an `@` in
    // a query string (e.g. `awskms:alias/x?ciphertext=base64@@==`) are
    // not mis-parsed. The `@status` suffix is positional and the only
    // legal place for it.
    let (head, status) = match spec.rsplit_once('@') {
        Some((h, s)) if matches!(s, "active" | "rotating" | "sunset") => (h, parse_status_word(s)?),
        // A status-shaped suffix (short, all-alphabetic) that isn't one
        // of the three valid words is almost certainly an operator typo
        // (e.g. `@actv`). Surfacing it here avoids the misleading
        // "file not found" the URI loader would otherwise produce when
        // it tries to open a path that ends in `@actv`.
        Some((_, s)) if is_status_shaped(s) => {
            return Err(format!(
                "invalid key status suffix '@{s}': expected '@active', '@rotating', or '@sunset'"
            )
            .into());
        }
        _ => (
            spec,
            if already_seeded {
                KeyStatus::Rotating
            } else {
                KeyStatus::Active
            },
        ),
    };
    let (uri, id) = match head.rsplit_once('#') {
        Some((u, i)) if !i.is_empty() => (u.to_string(), i.to_string()),
        _ => (head.to_string(), format!("key-{}", short_id_for_uri(head))),
    };
    Ok((uri, status, id))
}

/// `true` when `s` looks like a status word — short, non-empty, all
/// ASCII alphabetic. Used to distinguish a typo'd `@status` suffix
/// (e.g. `@actv`) from a legitimate non-status `@` inside a URI
/// (e.g. an `=`-padded base64 ciphertext tail).
fn is_status_shaped(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphabetic())
}

fn parse_status_word(s: &str) -> Result<KeyStatus, Box<dyn std::error::Error + Send + Sync>> {
    match s {
        "active" => Ok(KeyStatus::Active),
        "rotating" => Ok(KeyStatus::Rotating),
        "sunset" => Ok(KeyStatus::Sunset),
        other => Err(format!("invalid key status: {other}").into()),
    }
}

/// Stable 6-char id derived from the URI, used when the operator did
/// not pin an explicit `#id`. Keeps log lines unique across keys
/// without leaking URI internals.
///
/// Uses SHA-256 (not `DefaultHasher`) because the derived id is
/// referenced from Prometheus labels and operator workflows: a
/// toolchain upgrade silently switching the hash output would rename
/// every auto-derived key id and break metric continuity. SHA-256 is
/// stable across compiler versions by construction.
fn short_id_for_uri(uri: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(uri.as_bytes());
    // Six hex chars = three bytes of digest. That's 24 bits of entropy,
    // ample for the realistic ≤ 3-key set the rotation runbook
    // enforces; collisions only matter relative to the other auto-
    // derived ids in the same process.
    format!("{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2])
}

/// Strip the query string from a key URI before logging. AWS KMS URIs
/// carry the wrapped DEK as `?ciphertext=<base64>`; even though the
/// ciphertext is only useful with the KMS key, treating it as
/// log-grade plaintext is poor hygiene. Falls back to the original
/// string when no query is present.
fn sanitize_uri_for_log(uri: &str) -> String {
    match uri.split_once('?') {
        Some((head, _)) => format!("{head}?<redacted>"),
        None => uri.to_string(),
    }
}

/// Strip the `user:password@` userinfo from a DB connection URL so it can be
/// logged without leaking credentials. Falls back to the original string when
/// the URL doesn't follow the `scheme://userinfo@host` shape.
fn sanitize_db_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let Some((_userinfo, host)) = rest.split_once('@') else {
        return url.to_string();
    };
    format!("{}://{}", scheme, host)
}

#[cfg(test)]
mod tests {
    use super::{parse_key_uri_spec, sanitize_db_url, sanitize_uri_for_log, validate_zk_key_dir};
    use dp_crypto::KeyStatus;

    #[test]
    fn parse_key_uri_default_status_active_for_first() {
        let (uri, status, _id) = parse_key_uri_spec("file:/k.hex", false).unwrap();
        assert_eq!(uri, "file:/k.hex");
        assert_eq!(status, KeyStatus::Active);
    }

    #[test]
    fn parse_key_uri_default_status_rotating_for_subsequent() {
        let (_uri, status, _id) = parse_key_uri_spec("file:/k.hex", true).unwrap();
        assert_eq!(status, KeyStatus::Rotating);
    }

    #[test]
    fn parse_key_uri_explicit_status_suffix() {
        let (uri, status, _id) = parse_key_uri_spec("file:/k.hex@sunset", true).unwrap();
        assert_eq!(uri, "file:/k.hex");
        assert_eq!(status, KeyStatus::Sunset);
    }

    #[test]
    fn parse_key_uri_typo_status_rejected() {
        // The bug guard: `@actv` would otherwise be glued back onto the
        // URI and surface as a confusing "file not found" instead of
        // pointing the operator at the real mistake.
        let err = parse_key_uri_spec("file:/k.hex@actv", false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid key status suffix"), "got: {msg}");
        assert!(msg.contains("actv"), "got: {msg}");
    }

    #[test]
    fn parse_key_uri_base64_query_tail_not_misparsed() {
        // The `@` inside a base64-padded query string ends with non-
        // alphabetic chars, so it must fall through to the default
        // status path rather than tripping the typo guard.
        let (uri, status, _id) =
            parse_key_uri_spec("awskms:alias/x?ciphertext=base64@@==", false).unwrap();
        assert_eq!(uri, "awskms:alias/x?ciphertext=base64@@==");
        assert_eq!(status, KeyStatus::Active);
    }

    #[test]
    fn sanitize_uri_for_log_strips_query_string() {
        assert_eq!(
            sanitize_uri_for_log("awskms:alias/dp?ciphertext=AQID"),
            "awskms:alias/dp?<redacted>"
        );
    }

    #[test]
    fn sanitize_uri_for_log_passthrough_without_query() {
        assert_eq!(
            sanitize_uri_for_log("file:/etc/dp/key.hex"),
            "file:/etc/dp/key.hex"
        );
    }

    #[test]
    fn strips_credentials() {
        assert_eq!(
            sanitize_db_url("postgres://user:pw@host:5432/db"),
            "postgres://host:5432/db"
        );
    }

    #[test]
    fn passthrough_without_userinfo() {
        assert_eq!(sanitize_db_url("postgres://host/db"), "postgres://host/db");
    }

    #[test]
    fn passthrough_non_url() {
        assert_eq!(sanitize_db_url("nonsense"), "nonsense");
    }

    #[test]
    fn validate_zk_key_dir_missing_dir() {
        let err = validate_zk_key_dir("/nonexistent/path").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn validate_zk_key_dir_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_zk_key_dir(dir.path().to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn validate_zk_key_dir_ok() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["proving_key.bin", "verifying_key.bin", "keys_metadata.json"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        assert!(validate_zk_key_dir(dir.path().to_str().unwrap()).is_ok());
    }

    use super::seed_pairs_from_json;
    use dp_engine::Engine;
    use dp_event::MemStore;
    use std::sync::Arc;
    use std::time::Duration;

    fn fresh_engine() -> Engine {
        let store = Arc::new(MemStore::new());
        Engine::new(store, Duration::from_secs(1))
    }

    #[test]
    fn seed_pairs_from_json_empty_array_is_noop() {
        let engine = fresh_engine();
        seed_pairs_from_json(&engine, "[]").unwrap();
        assert!(engine.list_pairs().is_empty());
    }

    #[test]
    fn seed_pairs_from_json_invalid_json_errors() {
        let engine = fresh_engine();
        let err = seed_pairs_from_json(&engine, "not json").unwrap_err();
        assert!(err.to_string().contains("parse error"));
    }

    #[test]
    fn seed_pairs_from_json_registers_pair_with_defaults() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "0x0000000000000000000000000000000000000002"
        }]"#;
        seed_pairs_from_json(&engine, seed).unwrap();
        let pairs = engine.list_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "ETH/USDC");
        assert_eq!(pairs[0].1.min_order_size, rust_decimal::Decimal::ZERO);
        assert_eq!(pairs[0].1.tick_size, rust_decimal::Decimal::ZERO);
    }

    #[test]
    fn seed_pairs_from_json_registers_pair_with_all_fields() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "0x0000000000000000000000000000000000000002",
            "minOrderSize": "0.01",
            "tickSize": "0.05",
            "auctionIntervalMs": 7000
        }]"#;
        seed_pairs_from_json(&engine, seed).unwrap();
        let pairs = engine.list_pairs();
        assert_eq!(pairs.len(), 1);
        let (_, cfg) = &pairs[0];
        assert_eq!(cfg.min_order_size, "0.01".parse().unwrap());
        assert_eq!(cfg.tick_size, "0.05".parse().unwrap());
        assert_eq!(cfg.auction_interval, Some(Duration::from_millis(7000)));
    }

    #[test]
    fn seed_pairs_from_json_bad_base_token_errors() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "garbage",
            "quoteToken": "0x0000000000000000000000000000000000000002"
        }]"#;
        let err = seed_pairs_from_json(&engine, seed).unwrap_err();
        assert!(err.to_string().contains("bad base_token"));
    }

    #[test]
    fn seed_pairs_from_json_bad_quote_token_errors() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "garbage"
        }]"#;
        let err = seed_pairs_from_json(&engine, seed).unwrap_err();
        assert!(err.to_string().contains("bad quote_token"));
    }

    #[test]
    fn seed_pairs_from_json_bad_min_order_size_errors() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "0x0000000000000000000000000000000000000002",
            "minOrderSize": "not-a-number"
        }]"#;
        let err = seed_pairs_from_json(&engine, seed).unwrap_err();
        assert!(err.to_string().contains("bad min_order_size"));
    }

    #[test]
    fn seed_pairs_from_json_bad_tick_size_errors() {
        let engine = fresh_engine();
        let seed = r#"[{
            "pair": "ETH/USDC",
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "0x0000000000000000000000000000000000000002",
            "tickSize": "abc"
        }]"#;
        let err = seed_pairs_from_json(&engine, seed).unwrap_err();
        assert!(err.to_string().contains("bad tick_size"));
    }

    #[test]
    fn seed_pairs_from_json_duplicate_pair_errors() {
        let engine = fresh_engine();
        let seed = r#"[
            {"pair":"ETH/USDC","baseToken":"0x0000000000000000000000000000000000000001","quoteToken":"0x0000000000000000000000000000000000000002"},
            {"pair":"ETH/USDC","baseToken":"0x0000000000000000000000000000000000000001","quoteToken":"0x0000000000000000000000000000000000000002"}
        ]"#;
        let err = seed_pairs_from_json(&engine, seed).unwrap_err();
        assert!(err.to_string().contains("pair seed ETH/USDC"));
    }
}
