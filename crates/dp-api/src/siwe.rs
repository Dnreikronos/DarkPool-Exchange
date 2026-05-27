use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_primitives::Address;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// NonceStore
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct NonceStore {
    inner: Arc<Mutex<HashMap<String, Instant>>>,
    ttl: Duration,
}

impl NonceStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    pub fn generate(&self) -> String {
        let nonce = siwe::generate_nonce();
        self.inner.lock().insert(nonce.clone(), Instant::now());
        nonce
    }

    pub fn consume(&self, nonce: &str) -> bool {
        let mut map = self.inner.lock();
        match map.remove(nonce) {
            Some(created) => created.elapsed() <= self.ttl,
            None => false,
        }
    }

    pub fn evict_stale(&self) {
        let mut map = self.inner.lock();
        let ttl = self.ttl;
        map.retain(|_, created| created.elapsed() <= ttl);
    }

    pub fn start_cleanup(&self, cancel: CancellationToken, interval: Duration) {
        let store = Self {
            inner: Arc::clone(&self.inner),
            ttl: self.ttl,
        };
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = ticker.tick() => store.evict_stale(),
                }
            }
        });
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

// ---------------------------------------------------------------------------
// SIWE verification
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum SiweError {
    #[error("failed to parse SIWE message: {0}")]
    Parse(String),
    #[error("nonce invalid or expired")]
    NonceInvalid,
    #[error("SIWE message expired")]
    Expired,
    #[error("chain ID mismatch: expected {expected}, got {got}")]
    ChainMismatch { expected: u64, got: u64 },
    #[error("signature verification failed: {0}")]
    Verification(String),
}

pub fn verify_siwe_message(
    message_str: &str,
    signature: &[u8; 65],
    nonce_store: &NonceStore,
    expected_chain_id: Option<u64>,
) -> Result<Address, SiweError> {
    let message: siwe::Message =
        message_str.parse().map_err(|e| SiweError::Parse(format!("{e}")))?;

    if !nonce_store.consume(&message.nonce) {
        return Err(SiweError::NonceInvalid);
    }

    if let Some(expected) = expected_chain_id {
        if message.chain_id != expected {
            return Err(SiweError::ChainMismatch {
                expected,
                got: message.chain_id,
            });
        }
    }

    if let Some(ref exp) = message.expiration_time {
        if exp < &time::OffsetDateTime::now_utc() {
            return Err(SiweError::Expired);
        }
    }

    message
        .verify_eip191(signature)
        .map_err(|e| SiweError::Verification(format!("{e}")))?;

    Ok(Address::from_slice(&message.address))
}

// ---------------------------------------------------------------------------
// JWT
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: u64,
    pub exp: u64,
}

#[derive(Clone)]
pub struct JwtManager {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
    ttl: Duration,
}

impl std::fmt::Debug for JwtManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtManager")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl JwtManager {
    pub fn new(secret: &str, ttl: Duration) -> Self {
        Self {
            encoding: Arc::new(EncodingKey::from_secret(secret.as_bytes())),
            decoding: Arc::new(DecodingKey::from_secret(secret.as_bytes())),
            ttl,
        }
    }

    pub fn issue(&self, address: Address) -> Result<(String, u64), jsonwebtoken::errors::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let exp = now + self.ttl.as_secs();
        let claims = Claims {
            sub: format!("{:#x}", address),
            iat: now,
            exp,
        };
        let token = jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)?;
        Ok((token, exp))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let data = jsonwebtoken::decode::<Claims>(token, &self.decoding, &Validation::default())?;
        Ok(data.claims)
    }

    pub fn address_from_claims(claims: &Claims) -> Option<Address> {
        claims.sub.parse().ok()
    }
}

// ---------------------------------------------------------------------------
// Shared state for auth endpoints
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SiweState {
    pub nonce_store: Arc<NonceStore>,
    pub jwt_manager: Arc<JwtManager>,
    pub chain_id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_generate_and_consume() {
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate();
        assert!(store.consume(&nonce));
    }

    #[test]
    fn nonce_double_consume_fails() {
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate();
        assert!(store.consume(&nonce));
        assert!(!store.consume(&nonce));
    }

    #[test]
    fn nonce_expired_fails() {
        let store = NonceStore::new(Duration::from_secs(0));
        let nonce = store.generate();
        std::thread::sleep(Duration::from_millis(10));
        assert!(!store.consume(&nonce));
    }

    #[test]
    fn nonce_unknown_fails() {
        let store = NonceStore::new(Duration::from_secs(300));
        assert!(!store.consume("nonexistent"));
    }

    #[test]
    fn nonce_evict_stale() {
        let store = NonceStore::new(Duration::from_secs(0));
        store.generate();
        store.generate();
        std::thread::sleep(Duration::from_millis(10));
        store.evict_stale();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn jwt_issue_and_verify_round_trip() {
        let mgr = JwtManager::new("test-secret-32-bytes-long-xxxxx", Duration::from_secs(3600));
        let addr: Address = "0x6Da01670d8fc844e736095918bbE11fE8D564163"
            .parse()
            .unwrap();
        let (token, exp) = mgr.issue(addr).unwrap();
        assert!(exp > 0);
        let claims = mgr.verify(&token).unwrap();
        assert_eq!(claims.sub, format!("{:#x}", addr));
    }

    #[test]
    fn jwt_tampered_token_rejected() {
        let mgr = JwtManager::new("test-secret-32-bytes-long-xxxxx", Duration::from_secs(3600));
        let addr: Address = "0x6Da01670d8fc844e736095918bbE11fE8D564163"
            .parse()
            .unwrap();
        let (mut token, _) = mgr.issue(addr).unwrap();
        token.push('x');
        assert!(mgr.verify(&token).is_err());
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let mgr1 = JwtManager::new("secret-one-xxxxxxxxxxxxxxxxxxxxxxxx", Duration::from_secs(3600));
        let mgr2 = JwtManager::new("secret-two-xxxxxxxxxxxxxxxxxxxxxxxx", Duration::from_secs(3600));
        let addr: Address = "0x6Da01670d8fc844e736095918bbE11fE8D564163"
            .parse()
            .unwrap();
        let (token, _) = mgr1.issue(addr).unwrap();
        assert!(mgr2.verify(&token).is_err());
    }

    #[test]
    fn jwt_address_from_claims() {
        let claims = Claims {
            sub: "0x6da01670d8fc844e736095918bbe11fe8d564163".into(),
            iat: 0,
            exp: 0,
        };
        let addr = JwtManager::address_from_claims(&claims).unwrap();
        assert_eq!(format!("{:#x}", addr), "0x6da01670d8fc844e736095918bbe11fe8d564163");
    }
}
