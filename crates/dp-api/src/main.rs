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
use dp_event::{FileStore, MemStore, Store};
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

    let store: Arc<dyn Store> = if cfg.event_log.is_empty() {
        info!("event log: in-memory (not durable)");
        Arc::new(MemStore::new())
    } else {
        info!(path = %cfg.event_log, "event log: durable");
        Arc::new(FileStore::open(&cfg.event_log)?)
    };

    let engine = Engine::new(store.clone(), cfg.auction_interval);

    if !cfg.operator_key.is_empty() {
        let dec = EciesDecrypter::from_file(&cfg.operator_key)?;
        engine.set_decrypter(Arc::new(dec));
        info!(key = %cfg.operator_key, "decrypter: ECIES");
    } else {
        info!("decrypter: noop (set --operator-key to enable)");
    }

    if !cfg.aggregator_bin.is_empty() {
        let timeout = if cfg.aggregator_timeout.is_zero() {
            None
        } else {
            Some(cfg.aggregator_timeout)
        };
        let agg = SubprocessAggregator::new(std::path::Path::new(&cfg.aggregator_bin), timeout)?;
        engine.set_aggregator(Arc::new(agg));
        let submit_timeout = if cfg.submit_timeout.is_zero() {
            cfg.aggregator_timeout
        } else {
            cfg.submit_timeout
        };
        engine.set_submit_timeout(submit_timeout);
        info!(bin = %cfg.aggregator_bin, "aggregator: subprocess");
    } else {
        info!("aggregator: noop (set --aggregator-bin to enable)");
    }

    if !cfg.eth_rpc.is_empty() {
        // Eth submitter wiring requires an alloy provider builder. The current
        // dp-settlement API takes a generic Provider; bootstrap glue for that
        // path is deferred to a follow-up.
        warn!(
            rpc = %cfg.eth_rpc,
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
