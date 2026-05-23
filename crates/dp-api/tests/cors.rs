use axum::body::Body;
use axum::http::{header, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::time::Duration;
use tower::ServiceExt;
use tower_http::cors::{AllowOrigin, CorsLayer};

const ALLOWED_ORIGIN: &str = "http://localhost:3000";
const UNKNOWN_ORIGIN: &str = "http://evil.example.com";

fn build_cors_layer(origins: &[&str]) -> CorsLayer {
    let values: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(values))
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-api-key"),
        ])
        .expose_headers([HeaderName::from_static("x-request-id")])
        .max_age(Duration::from_secs(3600))
}

fn app_with_cors(origins: &[&str]) -> Router {
    Router::new()
        .route("/v1/orders", get(|| async { "ok" }).post(|| async { "ok" }))
        .route("/v1/pairs", get(|| async { "ok" }))
        .layer(build_cors_layer(origins))
}

fn app_without_cors() -> Router {
    Router::new()
        .route("/v1/orders", get(|| async { "ok" }).post(|| async { "ok" }))
        .route("/v1/pairs", get(|| async { "ok" }))
}

#[tokio::test]
async fn preflight_returns_cors_headers() {
    let app = app_with_cors(&[ALLOWED_ORIGIN]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v1/orders")
                .header("origin", ALLOWED_ORIGIN)
                .header("access-control-request-method", "POST")
                .header("access-control-request-headers", "x-api-key,content-type")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        ALLOWED_ORIGIN,
    );
    let methods = resp
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(methods.contains("POST"), "expected POST in {methods}");
    assert!(
        resp.headers().get("access-control-max-age").is_some(),
        "expected max-age header",
    );
}

#[tokio::test]
async fn actual_cross_origin_get_includes_allow_origin() {
    let app = app_with_cors(&[ALLOWED_ORIGIN]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/pairs")
                .header("origin", ALLOWED_ORIGIN)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        ALLOWED_ORIGIN,
    );
    let exposed = resp
        .headers()
        .get("access-control-expose-headers")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        exposed.contains("x-request-id"),
        "expected x-request-id in expose-headers, got {exposed}",
    );
}

#[tokio::test]
async fn unknown_origin_gets_no_allow_header() {
    let app = app_with_cors(&[ALLOWED_ORIGIN]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v1/orders")
                .header("origin", UNKNOWN_ORIGIN)
                .header("access-control-request-method", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "unknown origin must not receive allow-origin",
    );
}

#[tokio::test]
async fn disabled_when_no_origins_configured() {
    let app = app_without_cors();

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/pairs")
                .header("origin", ALLOWED_ORIGIN)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "no CORS headers when origins not configured",
    );
}

#[tokio::test]
async fn multiple_origins_allowed() {
    let second = "https://app.darkpool.exchange";
    let app = app_with_cors(&[ALLOWED_ORIGIN, second]);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/pairs")
                .header("origin", second)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        second,
    );
}

#[tokio::test]
async fn preflight_allows_patch_for_admin_routes() {
    let app = app_with_cors(&[ALLOWED_ORIGIN]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v1/orders")
                .header("origin", ALLOWED_ORIGIN)
                .header("access-control-request-method", "PATCH")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let methods = resp
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(methods.contains("PATCH"), "expected PATCH in {methods}");
}
