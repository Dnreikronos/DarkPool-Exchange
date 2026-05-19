use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use dp_aggregator::SubprocessAggregator;
use dp_api::admin::AdminApiHandler;
use dp_api::auth::{AuthCore, AuthLayer};
use dp_api::config::Config;
use dp_api::handler::ApiHandler;
use dp_api::observability::{self, M_EVENT_LOG_SIZE_BYTES};
use dp_api::pb::dark_pool_service_server::DarkPoolServiceServer;
use dp_api::ratelimit::{RateLimitCore, RateLimitLayer};
use dp_api::readiness::{aggregator_probe, store_probe, ReadinessProbes};
use dp_api::rest::{self, OpsState};
use dp_crypto::EciesDecrypter;
use dp_engine::{Engine, PairConfig, PairStatus};
use dp_event::{FileStore, MemStore, PgStore, Store};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Metrics first: the recorder must be installed before any
    // `metrics::counter!` call so the Prometheus exporter sees them.
    let prom_handle = observability::init_metrics()?;
    let tracing_guard = observability::init_tracing()?;

    let cfg = Config::parse();

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

    let engine = Engine::new(store.clone(), cfg.auction_interval);

    if let Some(key_path) = cfg.operator_key_path() {
        // Fail-fast: surface bad operator-key paths at boot rather than on
        // first decrypt attempt.
        let dec = EciesDecrypter::from_file(key_path)?;
        engine.set_decrypter(Arc::new(dec));
        info!(key = %key_path, "decrypter: ECIES");
    } else {
        info!("decrypter: noop (set --operator-key to enable)");
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
        // Eth submitter wiring requires an alloy provider builder. The current
        // dp-settlement API takes a generic Provider; bootstrap glue for that
        // path is deferred to a follow-up. The companion `eth_rpc` readiness
        // probe is deferred with it — adding a probe today would only HTTP-ping
        // a URL with no live provider behind it, which would mask a genuine
        // RPC outage once the submitter ships.
        warn!(
            rpc = %rpc,
            "DARKPOOL_ETH_RPC is set but submitter wiring is not yet implemented; \
             running with noop submitter — auctions will NOT be settled on-chain. \
             The `eth_rpc` readiness probe is also deferred until the submitter lands."
        );
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

    let auth_core = AuthCore::new(cfg.api_keys());
    let operator_keys = cfg.operator_api_keys();
    if operator_keys.is_empty() {
        warn!(
            "DARKPOOL_OPERATOR_API_KEYS is empty — admin endpoints will accept \
             unauthenticated requests. Set the env var before exposing the server."
        );
    }
    let admin_auth_core = AuthCore::new(operator_keys);
    let rl_core = RateLimitCore::new(cfg.rate_limit, cfg.rate_burst, cfg.rate_stale_after);
    rl_core.start_cleanup(cancel.clone(), Duration::from_secs(60));

    let auth = AuthLayer::from_core(auth_core.clone());
    let ratelimit = RateLimitLayer::from_core(rl_core.clone());

    let handler = ApiHandler::new(engine.clone());
    let admin_handler = AdminApiHandler::new(engine.clone());
    let shared = Arc::new(handler.clone());
    let shared_admin = Arc::new(admin_handler.clone());

    let grpc_cancel = cancel.clone();
    let grpc_addr = cfg.grpc_addr;
    let grpc_auth = auth.clone();
    let grpc_rl = ratelimit.clone();
    // The admin service is intentionally NOT mounted on the gRPC port:
    // the gRPC listener authenticates with the trader API key set, which
    // would let any trader call RegisterPair/SuspendPair/DelistPair if the
    // admin service were attached here. Admin RPCs ride on the REST
    // `/v1/admin/*` paths only (gated by the separate operator key set in
    // [`rest::router_with_admin`]). Restoring gRPC admin will require a
    // dedicated listener on its own port with the operator AuthLayer.
    let grpc_handle = tokio::spawn(async move {
        info!(addr = %grpc_addr, "gRPC server listening");
        Server::builder()
            .layer(grpc_auth)
            .layer(grpc_rl)
            .add_service(DarkPoolServiceServer::new(handler))
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

    let rest_app = rest::router_with_ops(
        shared,
        shared_admin,
        auth_core,
        admin_auth_core,
        rl_core,
        ops,
    );
    let http_addr = cfg.http_addr;
    let http_cancel = cancel.clone();
    let rest_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr)
            .await
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "bind {}: {}",
                    http_addr, e
                ))
            })?;
        info!(addr = %http_addr, "REST server listening");
        axum::serve(
            listener,
            rest_app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move { http_cancel.cancelled().await })
        .await
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))
    });

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
        let cfg = PairConfig {
            base_token: base,
            quote_token: quote,
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
    use super::{sanitize_db_url, validate_zk_key_dir};

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
