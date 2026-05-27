use dp_api::auth::AuthCore;
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
