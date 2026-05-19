//! Metrics + tracing wiring for the operator process.
//!
//! Two independently-installed globals:
//! - `metrics` recorder: a Prometheus text encoder reachable via the
//!   returned [`PrometheusHandle`]. Mounted on `/metrics`.
//! - `tracing` subscriber: env-filtered, JSON or text formatter, with an
//!   optional OpenTelemetry layer that exports OTLP/gRPC spans when
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
//!
//! Init order matters: the metrics recorder must be installed before any
//! `metrics::counter!` / `histogram!` call (otherwise descriptions are
//! lost). Tracing init runs second so OTel can pick up the same service
//! name env var.

use std::io::IsTerminal;

use metrics::{describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Config, TracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

// Metric-name constants live in `dp-types::metrics` so both this crate
// (descriptions + force-register) and `dp-engine` (emit sites) link the
// same strings. Re-exported here so existing callers (`main.rs`,
// integration tests) keep importing them through this module.
pub use dp_types::metrics::{
    M_ACTIVE_ORDERS, M_AUCTIONS_TOTAL, M_AUCTION_DURATION, M_BATCH_SUBMISSION_DURATION,
    M_CLEARING_PRICE, M_EVENT_LOG_SIZE_BYTES, M_ORDERS_EXPIRED, M_ORDERS_MATCHED, M_ORDERS_PLACED,
    M_SETTLEMENT_CONFIRMATIONS,
};

const AUCTION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
const BATCH_BUCKETS: &[f64] = &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0];

/// Install the global metrics recorder and pre-register every metric the
/// process emits, so `/metrics` returns the full schema even before the
/// first auction tick fires.
pub fn init_metrics() -> Result<PrometheusHandle, ObservabilityError> {
    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Full(M_AUCTION_DURATION.into()),
            AUCTION_BUCKETS,
        )
        .map_err(|e| ObservabilityError::Setup(e.to_string()))?
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Full(M_BATCH_SUBMISSION_DURATION.into()),
            BATCH_BUCKETS,
        )
        .map_err(|e| ObservabilityError::Setup(e.to_string()))?
        .install_recorder()
        .map_err(|e| ObservabilityError::Setup(e.to_string()))?;

    describe_counter!(
        M_AUCTIONS_TOTAL,
        "Auctions that produced at least one match"
    );
    describe_histogram!(
        M_AUCTION_DURATION,
        "Wall-clock duration of a single auction (matching algorithm only), seconds"
    );
    describe_counter!(
        M_ORDERS_PLACED,
        "Orders successfully placed into the book, labeled by side"
    );
    describe_counter!(
        M_ORDERS_MATCHED,
        "Matches produced in successful auctions (one increment per bid/ask pair)"
    );
    describe_counter!(M_ORDERS_EXPIRED, "Orders dropped because their TTL elapsed");
    describe_gauge!(
        M_CLEARING_PRICE,
        "Latest auction clearing price, labeled by pair"
    );
    describe_histogram!(
        M_BATCH_SUBMISSION_DURATION,
        "Wall-clock duration of submitBatch RPC round-trip (success path), seconds"
    );
    describe_counter!(
        M_SETTLEMENT_CONFIRMATIONS,
        "BatchSettled events observed by the on-chain watcher"
    );
    describe_gauge!(M_ACTIVE_ORDERS, "Orders currently resting in the book");
    describe_gauge!(
        M_EVENT_LOG_SIZE_BYTES,
        "Approximate size of the persistent event log, bytes"
    );

    // Force-register the families that may otherwise stay absent until
    // the first event of that kind, so `/metrics` is stable for scraping.
    metrics::counter!(M_AUCTIONS_TOTAL).absolute(0);
    metrics::counter!(M_ORDERS_MATCHED).absolute(0);
    metrics::counter!(M_ORDERS_EXPIRED).absolute(0);
    metrics::counter!(M_SETTLEMENT_CONFIRMATIONS).absolute(0);
    metrics::counter!(M_ORDERS_PLACED, "side" => "buy").absolute(0);
    metrics::counter!(M_ORDERS_PLACED, "side" => "sell").absolute(0);
    metrics::gauge!(M_ACTIVE_ORDERS).set(0.0);
    metrics::gauge!(M_EVENT_LOG_SIZE_BYTES).set(0.0);

    Ok(handle)
}

/// Tracing setup. Resolves the log format and OTLP endpoint from env;
/// the returned [`TracingGuard`] owns the OTel `TracerProvider` so the
/// caller can flush it on shutdown.
///
/// **Process-global.** `tracing_subscriber::registry().try_init()` installs
/// the dispatcher for the entire process, so a second call from the same
/// binary (e.g. an integration test spinning up a second server in-process)
/// returns [`ObservabilityError::Setup`] rather than re-initialising.
/// Wrap in a `OnceLock` if you need shared-init semantics.
pub fn init_tracing() -> Result<TracingGuard, ObservabilityError> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let want_json = resolve_want_json(
        std::env::var("DP_LOG_FORMAT").ok(),
        std::io::stdout().is_terminal(),
    );

    let otlp_endpoint = resolve_otlp_endpoint(std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok());

    let mut guard = TracingGuard::default();

    let otel_layer = if let Some(endpoint) = otlp_endpoint {
        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "dp-api".to_string());

        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .build_span_exporter()
            .map_err(|e| ObservabilityError::Setup(format!("otlp exporter: {e}")))?;

        let deployment_env = resolve_deployment_environment(
            std::env::var("DP_ENVIRONMENT").ok(),
            std::env::var("DEPLOYMENT_ENVIRONMENT").ok(),
        );
        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_config(Config::default().with_resource(Resource::new(vec![
                KeyValue::new("service.name", service_name.clone()),
                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                KeyValue::new("deployment.environment", deployment_env),
            ])))
            .build();

        let tracer = provider.tracer(service_name);
        guard.provider = Some(provider);
        Some(tracing_opentelemetry::layer().with_tracer(tracer))
    } else {
        None
    };

    // Box the format layer so the json and text branches produce a
    // single concrete type, otherwise `Subscriber::try_init` cannot be
    // called on the combined layer stack.
    let fmt_layer: Box<dyn Layer<_> + Send + Sync> = if want_json {
        Box::new(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(false),
        )
    } else {
        Box::new(tracing_subscriber::fmt::layer())
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .try_init()
        .map_err(|e| ObservabilityError::Setup(e.to_string()))?;

    Ok(guard)
}

/// RAII handle for OTel provider. Drop runs a best-effort flush so
/// in-flight spans aren't lost when the process exits.
#[derive(Default)]
pub struct TracingGuard {
    provider: Option<TracerProvider>,
}

impl TracingGuard {
    /// Explicit flush + shutdown. Call before exit when you want to wait
    /// for export. Dropping the guard does the same on a best-effort
    /// basis but cannot block.
    pub fn shutdown(mut self) {
        if let Some(provider) = self.provider.take() {
            for r in provider.force_flush() {
                if let Err(e) = r {
                    tracing::warn!("otel flush: {e:?}");
                }
            }
            // Provider drop triggers shutdown; explicit shutdown is API-
            // gated and not available on TracerProvider in 0.26. We never
            // register the provider via `global::set_tracer_provider`, so
            // `global::shutdown_tracer_provider()` would be a no-op here
            // and is intentionally omitted.
            drop(provider);
        }
    }
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            for r in provider.force_flush() {
                let _ = r;
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("observability setup failed: {0}")]
    Setup(String),
}

/// Map the `DP_LOG_FORMAT` env var to a JSON/text choice. Accepts
/// `json` → JSON, `text`/`plain` → text, anything else → auto-detect
/// (JSON when stdout is not a TTY). Extracted so unit tests can cover
/// each branch without touching the global subscriber.
fn resolve_want_json(env_value: Option<String>, stdout_is_tty: bool) -> bool {
    match env_value
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
    {
        Some(v) if v == "json" => true,
        Some(v) if v == "text" || v == "plain" => false,
        _ => !stdout_is_tty,
    }
}

/// Treat empty / whitespace-only OTLP endpoint env values as unset so
/// blank entries in a Kubernetes ConfigMap don't accidentally enable
/// the OTel layer.
fn resolve_otlp_endpoint(env_value: Option<String>) -> Option<String> {
    env_value.filter(|s| !s.trim().is_empty())
}

/// Resolve `deployment.environment` from `DP_ENVIRONMENT`, falling
/// back to `DEPLOYMENT_ENVIRONMENT`, defaulting to "development".
fn resolve_deployment_environment(primary: Option<String>, fallback: Option<String>) -> String {
    primary
        .or(fallback)
        .unwrap_or_else(|| "development".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Prometheus recorder is a process-global; the test runner may
    // invoke `init_metrics` only once. We gate behind a OnceLock so
    // multiple tests can share the handle without panicking on the
    // "global recorder already set" error.
    use std::sync::OnceLock;
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

    fn shared_handle() -> &'static PrometheusHandle {
        HANDLE.get_or_init(|| init_metrics().expect("init_metrics"))
    }

    #[test]
    fn renders_every_metric_family() {
        let h = shared_handle();
        let out = h.render();
        for name in [
            M_AUCTIONS_TOTAL,
            M_ORDERS_PLACED,
            M_ORDERS_MATCHED,
            M_ORDERS_EXPIRED,
            M_SETTLEMENT_CONFIRMATIONS,
            M_ACTIVE_ORDERS,
            M_EVENT_LOG_SIZE_BYTES,
        ] {
            assert!(
                out.contains(name),
                "expected /metrics to contain {name} after init, body=\n{out}"
            );
        }
        // Histograms / per-pair gauges only show up after a value is
        // recorded, by design of the prometheus client.
    }

    #[test]
    fn histogram_and_clearing_price_appear_after_first_observation() {
        let _ = shared_handle();
        metrics::histogram!(M_AUCTION_DURATION).record(0.012);
        metrics::histogram!(M_BATCH_SUBMISSION_DURATION).record(0.42);
        metrics::gauge!(M_CLEARING_PRICE, "pair" => "ETH/USDC").set(2000.0);

        let out = HANDLE.get().unwrap().render();
        assert!(out.contains(M_AUCTION_DURATION));
        assert!(out.contains(M_BATCH_SUBMISSION_DURATION));
        assert!(out.contains(M_CLEARING_PRICE));
        assert!(out.contains("pair=\"ETH/USDC\""));
    }

    #[test]
    fn want_json_explicit_json() {
        assert!(resolve_want_json(Some("json".into()), true));
        assert!(resolve_want_json(Some("JSON".into()), true));
        assert!(resolve_want_json(Some(" json ".into()), true));
    }

    #[test]
    fn want_json_explicit_text() {
        assert!(!resolve_want_json(Some("text".into()), false));
        assert!(!resolve_want_json(Some("plain".into()), false));
        assert!(!resolve_want_json(Some("PLAIN".into()), false));
    }

    #[test]
    fn want_json_unset_follows_tty() {
        // No env override → JSON when stdout is piped, text when on a TTY.
        assert!(resolve_want_json(None, false));
        assert!(!resolve_want_json(None, true));
    }

    #[test]
    fn want_json_unknown_value_falls_back_to_tty() {
        // Unrecognised value must not silently default to a fixed
        // format; behaviour should match the unset case.
        assert!(resolve_want_json(Some("yaml".into()), false));
        assert!(!resolve_want_json(Some("yaml".into()), true));
    }

    #[test]
    fn otlp_endpoint_passes_through_when_set() {
        assert_eq!(
            resolve_otlp_endpoint(Some("http://collector:4317".into())),
            Some("http://collector:4317".into()),
        );
    }

    #[test]
    fn otlp_endpoint_treats_blank_as_unset() {
        assert_eq!(resolve_otlp_endpoint(None), None);
        assert_eq!(resolve_otlp_endpoint(Some(String::new())), None);
        assert_eq!(resolve_otlp_endpoint(Some("   ".into())), None);
        assert_eq!(resolve_otlp_endpoint(Some("\t\n".into())), None);
    }

    #[test]
    fn deployment_env_prefers_primary() {
        assert_eq!(
            resolve_deployment_environment(Some("staging".into()), Some("prod".into())),
            "staging",
        );
    }

    #[test]
    fn deployment_env_falls_back_when_primary_unset() {
        assert_eq!(
            resolve_deployment_environment(None, Some("prod".into())),
            "prod",
        );
    }

    #[test]
    fn deployment_env_defaults_to_development() {
        assert_eq!(resolve_deployment_environment(None, None), "development",);
    }

    #[test]
    fn observability_error_display_includes_inner_message() {
        let err = ObservabilityError::Setup("recorder already set".into());
        let rendered = err.to_string();
        assert!(rendered.contains("recorder already set"), "got: {rendered}");
        assert!(rendered.contains("observability setup failed"));
    }

    #[test]
    fn tracing_guard_default_shutdown_is_no_op() {
        // A default-constructed guard has no provider, so shutdown and
        // drop must both be safe to call without an OTel exporter wired.
        let guard = TracingGuard::default();
        guard.shutdown();
    }

    #[test]
    fn tracing_guard_default_drop_is_no_op() {
        // Drop should not panic when provider is None.
        let _ = TracingGuard::default();
    }
}
