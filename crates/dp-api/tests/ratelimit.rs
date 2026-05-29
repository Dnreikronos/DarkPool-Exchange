use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use dp_api::ratelimit::{client_key, RateLimitCore};
use http::{HeaderMap, HeaderValue};
use tonic::Code;

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
