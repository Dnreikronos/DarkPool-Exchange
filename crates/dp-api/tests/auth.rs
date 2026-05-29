use std::sync::Arc;
use std::time::Duration;

use dp_api::auth::{AuthCore, AuthenticatedIdentity};
use dp_api::siwe::JwtManager;
use http::{HeaderMap, HeaderValue};
use tonic::Code;

#[test]
fn no_keys_pass_through() {
    let core = AuthCore::new(vec![]);
    let h = HeaderMap::new();
    assert!(core.check(&h).is_ok());
}

#[test]
fn valid_key_passes() {
    let core = AuthCore::new(vec!["k1".into(), "k2".into()]);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("k1"));
    assert!(core.check(&h).is_ok());
}

#[test]
fn invalid_key_denied() {
    let core = AuthCore::new(vec!["k1".into()]);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("bad"));
    let err = core.check(&h).err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
}

#[test]
fn missing_header_unauthenticated() {
    let core = AuthCore::new(vec!["k1".into()]);
    let h = HeaderMap::new();
    let err = core.check(&h).err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
}

#[test]
fn empty_keys_filter() {
    let core = AuthCore::new(vec!["".into(), "k1".into()]);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static(""));
    let err = core.check(&h).err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
}

fn jwt_manager() -> Arc<JwtManager> {
    Arc::new(JwtManager::new(
        "test-secret-32-bytes-long-xxxxx",
        Duration::from_secs(3600),
    ))
}

#[test]
fn bearer_token_returns_wallet_identity() {
    let jwt = jwt_manager();
    let addr: alloy_primitives::Address = "0x6Da01670d8fc844e736095918bbE11fE8D564163"
        .parse()
        .unwrap();
    let (token, _) = jwt.issue(addr).unwrap();
    let core = AuthCore::new_with_jwt(vec!["k1".into()], jwt);
    let mut h = HeaderMap::new();
    h.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    match core.check(&h).unwrap() {
        AuthenticatedIdentity::Wallet(a) => assert_eq!(a, addr),
        other => panic!("expected Wallet, got {other:?}"),
    }
}

#[test]
fn bearer_rejected_when_siwe_disabled() {
    let core = AuthCore::new(vec!["k1".into()]);
    let mut h = HeaderMap::new();
    h.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("Bearer fake.jwt.token"),
    );
    let err = core.check(&h).err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
    assert!(
        err.message().contains("not enabled"),
        "msg: {}",
        err.message()
    );
}

#[test]
fn invalid_bearer_token_rejected() {
    let jwt = jwt_manager();
    let core = AuthCore::new_with_jwt(vec![], jwt);
    let mut h = HeaderMap::new();
    h.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("Bearer invalid.token.here"),
    );
    let err = core.check(&h).err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
}

#[test]
fn api_key_still_works_with_jwt_enabled() {
    let jwt = jwt_manager();
    let core = AuthCore::new_with_jwt(vec!["k1".into()], jwt);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("k1"));
    match core.check(&h).unwrap() {
        AuthenticatedIdentity::ApiKey => {}
        other => panic!("expected ApiKey, got {other:?}"),
    }
}
