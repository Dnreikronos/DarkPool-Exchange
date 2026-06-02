use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dp_api::rest::auth_router;
use dp_api::siwe::{JwtManager, NonceStore, SiweState};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn siwe_app() -> (axum::Router, SiweState) {
    let nonce_store = Arc::new(NonceStore::new(Duration::from_secs(300)));
    let jwt_manager = Arc::new(JwtManager::new(
        "test-secret-32-bytes-long-xxxxx",
        Duration::from_secs(3600),
    ));
    let state = SiweState {
        nonce_store,
        jwt_manager,
        chain_id: None,
        expected_domain: None,
    };
    (auth_router(state.clone()), state)
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test]
async fn nonce_returns_200_with_nonce_string() {
    let (app, _) = siwe_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/auth/nonce")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert!(v["nonce"].is_string());
    assert!(!v["nonce"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn verify_rejects_invalid_signature_hex() {
    let (app, _) = siwe_app();
    let body = serde_json::json!({
        "message": "test",
        "signature": "not-hex"
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn verify_rejects_wrong_length_signature() {
    let (app, _) = siwe_app();
    let body = serde_json::json!({
        "message": "test",
        "signature": "0xaabb"
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn verify_rejects_unparseable_siwe_message() {
    let (app, _) = siwe_app();
    let sig_hex = format!("0x{}", "ab".repeat(65));
    let body = serde_json::json!({
        "message": "not a SIWE message",
        "signature": sig_hex
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn nonce_store_cap_returns_429() {
    let nonce_store = Arc::new(NonceStore::new(Duration::from_secs(300)));
    for _ in 0..10_000 {
        nonce_store.generate();
    }
    let state = SiweState {
        nonce_store,
        jwt_manager: Arc::new(JwtManager::new(
            "test-secret-32-bytes-long-xxxxx",
            Duration::from_secs(3600),
        )),
        chain_id: None,
        expected_domain: None,
    };
    let app = auth_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/auth/nonce")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn nonce_rate_limited_per_ip_end_to_end() {
    use axum::extract::ConnectInfo;
    use axum::middleware::from_fn_with_state;
    use dp_api::ratelimit::{ratelimit_ip_axum_mw, RateLimitCore};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    let (router, _state) = siwe_app();
    // rate ~0, burst 1 → one nonce per IP, then throttled.
    let core = RateLimitCore::new(0.0001, 1.0, Duration::from_secs(60));
    let app = router.layer(from_fn_with_state(core, ratelimit_ip_axum_mw));

    let nonce_req = |ip: [u8; 4]| {
        let mut req = Request::builder()
            .method("GET")
            .uri("/v1/auth/nonce")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), 1234);
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    };

    // IP-A: first succeeds, second is throttled before touching the store.
    let resp = app.clone().oneshot(nonce_req([1, 1, 1, 1])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app.clone().oneshot(nonce_req([1, 1, 1, 1])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    // A different IP is unaffected — no global login DoS.
    let resp = app.clone().oneshot(nonce_req([2, 2, 2, 2])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
