//! Integration tests for the unauthenticated ops endpoints and the
//! request-id middleware that wraps every route.

use std::sync::Arc;
use std::sync::OnceLock;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use dp_api::admin::{AdminApiHandler, KeyAdminHandler};
use dp_api::auth::AuthCore;
use dp_api::handler::ApiHandler;
use dp_api::observability::{init_metrics, M_AUCTIONS_TOTAL, M_ORDERS_PLACED};
use dp_api::ratelimit::RateLimitCore;
use dp_api::readiness::{Probe, ReadinessProbes};
use dp_api::rest::{router_with_ops, OpsState};
use dp_crypto::MultiKeyDecrypter;
use dp_engine::Engine;
use dp_event::{MemStore, Store};
use metrics_exporter_prometheus::PrometheusHandle;
use std::time::Duration;
use tower::ServiceExt;

// Prometheus recorder is process-global; the metrics tests share one
// handle so concurrent tests in this binary don't race the
// "global recorder already set" check.
static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
fn shared_prom() -> PrometheusHandle {
    HANDLE
        .get_or_init(|| init_metrics().expect("init_metrics"))
        .clone()
}

fn fresh_engine() -> Engine {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    Engine::new(store, Duration::from_secs(60))
}

fn make_router(probes: ReadinessProbes) -> axum::Router {
    let engine = fresh_engine();
    let api = ApiHandler::new(engine.clone());
    let admin = AdminApiHandler::new(engine);
    let key_admin = KeyAdminHandler::new(MultiKeyDecrypter::new());
    let auth = AuthCore::new(vec!["trader-key".into()]);
    let admin_auth = AuthCore::new(vec!["admin-key".into()]);
    let rl = RateLimitCore::new(1000.0, 1000.0, std::time::Duration::from_secs(60));
    router_with_ops(
        Arc::new(api),
        Arc::new(admin),
        Arc::new(key_admin),
        auth,
        admin_auth,
        rl,
        OpsState {
            prom: shared_prom(),
            readiness: probes,
            multi: dp_crypto::MultiKeyDecrypter::new(),
        },
        &[],
        None,
    )
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8 body")
}

#[tokio::test]
async fn healthz_returns_200_without_auth() {
    let router = make_router(ReadinessProbes::new());
    let resp = router
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn readyz_returns_200_when_probes_pass() {
    let probes = ReadinessProbes::new()
        .with(Probe::new("always_ok", || Ok(())))
        .with(Probe::new("also_ok", || Ok(())));
    let router = make_router(probes);
    let resp = router
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("\"status\":\"ready\""), "body={body}");
}

#[tokio::test]
async fn readyz_returns_503_when_probe_fails() {
    let probes = ReadinessProbes::new()
        .with(Probe::new("first", || Ok(())))
        .with(Probe::new("event_store", || Err("db unreachable".into())));
    let router = make_router(probes);
    let resp = router
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body_string(resp).await;
    assert!(body.contains("event_store"), "body={body}");
    // Probe `reason` must NOT appear in the response: `/readyz` is
    // unauthenticated, so the detailed reason is logged server-side
    // instead of returned to the client.
    assert!(
        !body.contains("db unreachable"),
        "probe reason leaked to /readyz body: {body}"
    );
}

#[tokio::test]
async fn metrics_returns_pre_registered_families() {
    let router = make_router(ReadinessProbes::new());
    let resp = router
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("content-type")
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/plain"), "content-type was {ct}");
    let body = body_string(resp).await;
    assert!(
        body.contains(M_AUCTIONS_TOTAL),
        "missing {M_AUCTIONS_TOTAL} in body={body}"
    );
    assert!(body.contains(M_ORDERS_PLACED));
}

#[tokio::test]
async fn request_id_is_generated_when_client_omits_header() {
    let router = make_router(ReadinessProbes::new());
    let resp = router
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id header must be set on the response");
    assert!(!id.is_empty(), "request id should not be empty");
}

#[tokio::test]
async fn request_id_is_propagated_when_client_supplies_it() {
    let router = make_router(ReadinessProbes::new());
    let resp = router
        .oneshot(
            Request::get("/healthz")
                .header("x-request-id", "trace-abc-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id must round-trip")
        .to_str()
        .unwrap();
    assert_eq!(id, "trace-abc-123");
}

#[tokio::test]
async fn request_id_is_present_on_error_responses() {
    let router = make_router(ReadinessProbes::new());
    // Hit an auth-gated route without a key → 401. The x-request-id
    // middleware wraps the entire router so the header must still appear.
    let resp = router
        .oneshot(
            Request::get("/v1/orderbook?pair=ETH/USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id must be set even on error responses")
        .to_str()
        .unwrap();
    assert!(!id.is_empty());
    // Verify the generated ID is a valid ULID.
    ulid::Ulid::from_string(id).expect("request id should be a valid ULID");
}

#[tokio::test]
async fn request_id_generated_is_valid_ulid() {
    let router = make_router(ReadinessProbes::new());
    let resp = router
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id header must be set")
        .to_str()
        .unwrap();
    ulid::Ulid::from_string(id).expect("generated request id should be a valid ULID");
}

#[tokio::test]
async fn ops_endpoints_skip_auth_while_protected_routes_still_require_it() {
    let router = make_router(ReadinessProbes::new());
    // No api key → /v1/orderbook must reject.
    let resp = router
        .clone()
        .oneshot(
            Request::get("/v1/orderbook?pair=ETH/USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "auth-gated routes still reject after middleware reshuffle"
    );

    // But /healthz must be open.
    let resp = router
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
