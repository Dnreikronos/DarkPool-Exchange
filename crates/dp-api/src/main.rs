use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use dp_aggregator::SubprocessAggregator;
use dp_api::auth::{AuthCore, AuthLayer};
use dp_api::config::Config;
use dp_api::handler::ApiHandler;
use dp_api::pb::dark_pool_service_server::DarkPoolServiceServer;
use dp_api::ratelimit::{RateLimitCore, RateLimitLayer};
use dp_api::rest;
use dp_crypto::EciesDecrypter;
use dp_engine::Engine;
use dp_event::{FileStore, MemStore, PgStore, Store};
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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
            return Err("DARKPOOL_ZK_PROVING_KEY must be set when DARKPOOL_AGGREGATOR_BIN is set"
                .into());
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
        // path is deferred to a follow-up.
        warn!(
            rpc = %rpc,
            "DARKPOOL_ETH_RPC is set but submitter wiring is not yet implemented; \
             running with noop submitter — auctions will NOT be settled on-chain"
        );
    } else {
        info!("batch submitter: noop (set --eth-rpc to enable on-chain settlement)");
    }

    engine.recover().await?;

    let cancel = CancellationToken::new();

    let engine_tick = engine.clone();
    let tick_cancel = cancel.clone();
    let engine_handle = tokio::spawn(async move {
        engine_tick.start(tick_cancel).await;
    });

    let auth_core = AuthCore::new(cfg.api_keys());
    let rl_core = RateLimitCore::new(cfg.rate_limit, cfg.rate_burst, cfg.rate_stale_after);
    rl_core.start_cleanup(cancel.clone(), Duration::from_secs(60));

    let auth = AuthLayer::from_core(auth_core.clone());
    let ratelimit = RateLimitLayer::from_core(rl_core.clone());

    let handler = ApiHandler::new(engine.clone());
    let shared = Arc::new(handler.clone());

    let grpc_cancel = cancel.clone();
    let grpc_addr = cfg.grpc_addr;
    let grpc_auth = auth.clone();
    let grpc_rl = ratelimit.clone();
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

    let rest_app = rest::router_with_middleware(shared, auth_core, rl_core);
    let http_addr = cfg.http_addr;
    let http_cancel = cancel.clone();
    let rest_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!("bind {}: {}", http_addr, e))
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
    use super::sanitize_db_url;

    #[test]
    fn strips_credentials() {
        assert_eq!(
            sanitize_db_url("postgres://user:pw@host:5432/db"),
            "postgres://host:5432/db"
        );
    }

    #[test]
    fn passthrough_without_userinfo() {
        assert_eq!(
            sanitize_db_url("postgres://host/db"),
            "postgres://host/db"
        );
    }

    #[test]
    fn passthrough_non_url() {
        assert_eq!(sanitize_db_url("nonsense"), "nonsense");
    }
}
