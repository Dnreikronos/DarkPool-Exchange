use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::Router;
use dp_api::ratelimit::{client_key, ip_client_key, ratelimit_ip_axum_mw, RateLimitCore};
use http::{HeaderMap, HeaderValue, Request, StatusCode};
use tonic::Code;
use tower::ServiceExt;

#[test]
fn allow_within_burst() {
    let r = RateLimitCore::new(1.0, 3.0, Duration::from_secs(60));
    for _ in 0..3 {
        assert!(r.allow("c").is_ok());
    }
}

#[test]
fn block_when_exhausted() {
    let r = RateLimitCore::new(0.0001, 2.0, Duration::from_secs(60));
    assert!(r.allow("c").is_ok());
    assert!(r.allow("c").is_ok());
    let err = r.allow("c").err().unwrap();
    assert_eq!(err.code(), Code::ResourceExhausted);
}

#[tokio::test]
async fn refill_over_time() {
    let r = RateLimitCore::new(100.0, 1.0, Duration::from_secs(60));
    assert!(r.allow("c").is_ok());
    assert!(r.allow("c").is_err()); // empty
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(r.allow("c").is_ok()); // refilled
}

#[tokio::test]
async fn capped_at_capacity() {
    let r = RateLimitCore::new(1000.0, 2.0, Duration::from_secs(60));
    assert!(r.allow("c").is_ok());
    assert!(r.allow("c").is_ok());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(r.allow("c").is_ok());
    assert!(r.allow("c").is_ok());
    assert!(r.allow("c").is_err()); // cap = 2 not > 2
}

#[test]
fn per_client_buckets() {
    let r = RateLimitCore::new(0.0001, 1.0, Duration::from_secs(60));
    assert!(r.allow("a").is_ok());
    assert!(r.allow("b").is_ok());
    assert!(r.allow("a").is_err());
    assert!(r.allow("b").is_err());
}

#[test]
fn key_precedence_api_key() {
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("kkk"));
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    assert_eq!(client_key(None, &h, peer), "kkk");
}

#[test]
fn key_precedence_peer_when_no_header() {
    let h = HeaderMap::new();
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    assert_eq!(client_key(None, &h, peer), "1.2.3.4");
}

#[test]
fn key_precedence_anonymous() {
    let h = HeaderMap::new();
    assert_eq!(client_key(None, &h, None), "anonymous");
}

#[test]
fn key_precedence_wallet_identity() {
    use dp_api::auth::AuthenticatedIdentity;
    let addr: alloy_primitives::Address = "0x6Da01670d8fc844e736095918bbE11fE8D564163"
        .parse()
        .unwrap();
    let identity = AuthenticatedIdentity::Wallet(addr);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("kkk"));
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    let key = client_key(Some(&identity), &h, peer);
    assert_eq!(key, format!("{:#x}", addr));
}

#[test]
fn key_precedence_apikey_identity_falls_through() {
    use dp_api::auth::AuthenticatedIdentity;
    let identity = AuthenticatedIdentity::ApiKey;
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("kkk"));
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    assert_eq!(client_key(Some(&identity), &h, peer), "kkk");
}

#[tokio::test]
async fn evict_stale() {
    let r = RateLimitCore::new(1.0, 1.0, Duration::from_millis(50));
    assert!(r.allow("a").is_ok());
    assert_eq!(r.bucket_count(), 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.evict_stale();
    assert_eq!(r.bucket_count(), 0);
}

#[tokio::test]
async fn keeps_fresh() {
    let r = RateLimitCore::new(1.0, 1.0, Duration::from_secs(10));
    assert!(r.allow("a").is_ok());
    r.evict_stale();
    assert_eq!(r.bucket_count(), 1);
}

// ---------- IP-only keying (unauthenticated routes) ----------

#[test]
fn ip_key_uses_peer_ip() {
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(9, 8, 7, 6)), 5555));
    assert_eq!(ip_client_key(peer), "9.8.7.6");
}

#[test]
fn ip_key_anonymous_without_peer() {
    assert_eq!(ip_client_key(None), "anonymous");
}

fn ip_app(core: RateLimitCore) -> Router {
    Router::new()
        .route("/x", get(|| async { "ok" }))
        .layer(from_fn_with_state(core, ratelimit_ip_axum_mw))
}

fn ip_req(ip: [u8; 4], api_key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri("/x");
    if let Some(k) = api_key {
        builder = builder.header("x-api-key", k);
    }
    let mut req = builder.body(Body::empty()).unwrap();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), 1234);
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

#[tokio::test]
async fn ip_mw_blocks_same_ip_after_burst() {
    // rate ~0, burst 2 → two pass, third is throttled.
    let app = ip_app(RateLimitCore::new(0.0001, 2.0, Duration::from_secs(60)));
    for _ in 0..2 {
        let resp = app
            .clone()
            .oneshot(ip_req([1, 1, 1, 1], None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let resp = app
        .clone()
        .oneshot(ip_req([1, 1, 1, 1], None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn ip_mw_separates_distinct_ips() {
    let app = ip_app(RateLimitCore::new(0.0001, 1.0, Duration::from_secs(60)));
    let resp = app
        .clone()
        .oneshot(ip_req([1, 1, 1, 1], None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .clone()
        .oneshot(ip_req([1, 1, 1, 1], None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    // A different peer IP has its own bucket.
    let resp = app
        .clone()
        .oneshot(ip_req([2, 2, 2, 2], None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ip_mw_ignores_api_key_header() {
    // Same IP, different `x-api-key` values must share ONE bucket — the
    // header must not let an unauthenticated caller mint extra buckets.
    let app = ip_app(RateLimitCore::new(0.0001, 1.0, Duration::from_secs(60)));
    let resp = app
        .clone()
        .oneshot(ip_req([3, 3, 3, 3], Some("key-a")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .clone()
        .oneshot(ip_req([3, 3, 3, 3], Some("key-b")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}
