use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::Router;
use dp_api::ratelimit::{
    client_key, ip_client_key, ratelimit_ip_axum_mw, RateLimitCore, TrustedProxies,
};
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
    assert_eq!(client_key(None, &h, peer, &TrustedProxies::none()), "kkk");
}

#[test]
fn key_precedence_peer_when_no_header() {
    let h = HeaderMap::new();
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    assert_eq!(
        client_key(None, &h, peer, &TrustedProxies::none()),
        "1.2.3.4"
    );
}

#[test]
fn key_precedence_anonymous() {
    let h = HeaderMap::new();
    assert_eq!(
        client_key(None, &h, None, &TrustedProxies::none()),
        "anonymous"
    );
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
    let key = client_key(Some(&identity), &h, peer, &TrustedProxies::none());
    assert_eq!(key, format!("{:#x}", addr));
}

#[test]
fn key_precedence_apikey_identity_falls_through() {
    use dp_api::auth::AuthenticatedIdentity;
    let identity = AuthenticatedIdentity::ApiKey;
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("kkk"));
    let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234));
    assert_eq!(
        client_key(Some(&identity), &h, peer, &TrustedProxies::none()),
        "kkk"
    );
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
    assert_eq!(
        ip_client_key(&TrustedProxies::none(), &HeaderMap::new(), peer),
        "9.8.7.6"
    );
}

#[test]
fn ip_key_anonymous_without_peer() {
    assert_eq!(
        ip_client_key(&TrustedProxies::none(), &HeaderMap::new(), None),
        "anonymous"
    );
}

// ---------- trusted-proxy / X-Forwarded-For resolution ----------

fn xff(value: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
    h
}

fn peer(ip: [u8; 4]) -> Option<SocketAddr> {
    Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])),
        1234,
    ))
}

#[test]
fn xff_ignored_when_no_trusted_proxies() {
    // Directly exposed: the forwarding header is attacker-controlled, so
    // the peer IP must win regardless of what XFF claims.
    let trusted = TrustedProxies::none();
    let h = xff("9.9.9.9");
    assert_eq!(
        ip_client_key(&trusted, &h, peer([1, 2, 3, 4])),
        "1.2.3.4",
        "XFF must be ignored without a trusted proxy"
    );
}

#[test]
fn xff_ignored_when_peer_is_not_trusted() {
    // A trust list exists, but this peer isn't on it → still ignore XFF.
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = xff("9.9.9.9");
    assert_eq!(ip_client_key(&trusted, &h, peer([1, 2, 3, 4])), "1.2.3.4");
}

#[test]
fn xff_honored_when_peer_is_trusted_proxy() {
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = xff("203.0.113.7");
    assert_eq!(
        ip_client_key(&trusted, &h, peer([10, 1, 2, 3])),
        "203.0.113.7",
        "behind a trusted proxy the real client IP must be used"
    );
}

#[test]
fn xff_uses_rightmost_untrusted_hop() {
    // Chain: real client, then two of our trusted proxies appended it.
    // The rightmost entry NOT in the trust set is the real client.
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = xff("203.0.113.7, 10.0.0.9, 10.0.0.1");
    assert_eq!(
        ip_client_key(&trusted, &h, peer([10, 0, 0, 1])),
        "203.0.113.7"
    );
}

#[test]
fn xff_spoof_does_not_extend_the_chain() {
    // Attacker prepends a forged hop ("1.1.1.1, 2.2.2.2") hoping to be keyed
    // as 1.1.1.1. The rightmost *untrusted* entry (2.2.2.2) wins, so padding
    // the chain with forged hops on the left cannot move the caller into a
    // different bucket. (Whether 2.2.2.2 itself is forgeable depends on the
    // proxy appending the real peer rather than passing this header through
    // — that invariant is locked by `xff_proxy_appended_client_defeats_spoof`.)
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = xff("1.1.1.1, 2.2.2.2");
    assert_eq!(ip_client_key(&trusted, &h, peer([10, 0, 0, 1])), "2.2.2.2");
}

#[test]
fn x_real_ip_fallback_when_no_xff() {
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let mut h = HeaderMap::new();
    h.insert("x-real-ip", HeaderValue::from_static("198.51.100.4"));
    assert_eq!(
        ip_client_key(&trusted, &h, peer([10, 0, 0, 1])),
        "198.51.100.4"
    );
}

#[test]
fn trusted_proxy_with_no_forwarding_header_falls_back_to_peer() {
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = HeaderMap::new();
    assert_eq!(ip_client_key(&trusted, &h, peer([10, 0, 0, 1])), "10.0.0.1");
}

#[test]
fn xff_strips_port_suffix() {
    let trusted = TrustedProxies::parse("10.0.0.0/8").unwrap();
    let h = xff("203.0.113.7:51000");
    assert_eq!(
        ip_client_key(&trusted, &h, peer([10, 0, 0, 1])),
        "203.0.113.7"
    );
}

#[test]
fn trusted_proxies_parse_rejects_garbage() {
    assert!(TrustedProxies::parse("not-an-ip").is_err());
    assert!(TrustedProxies::parse("10.0.0.0/99").is_err());
    // Blank / whitespace-only specs are the directly-exposed default.
    assert!(TrustedProxies::parse("  ").unwrap().is_empty());
    assert!(TrustedProxies::parse("").unwrap().is_empty());
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
