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

const MAX_NONCES: usize = 10_000;

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

    pub fn generate(&self) -> Option<String> {
        let mut map = self.inner.lock();
        if map.len() >= MAX_NONCES {
            return None;
        }
        let nonce = siwe::generate_nonce();
        map.insert(nonce.clone(), Instant::now());
        Some(nonce)
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
    #[error("SIWE message not yet valid")]
    NotYetValid,
    #[error("domain mismatch: expected {expected}, got {got}")]
    DomainMismatch { expected: String, got: String },
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
    expected_domain: Option<&str>,
) -> Result<Address, SiweError> {
    let message: siwe::Message = message_str
        .parse()
        .map_err(|e| SiweError::Parse(format!("{e}")))?;

    if let Some(domain) = expected_domain {
        let msg_domain = message.domain.to_string();
        if msg_domain != domain {
            return Err(SiweError::DomainMismatch {
                expected: domain.to_string(),
                got: msg_domain,
            });
        }
    }

    if let Some(expected) = expected_chain_id {
        if message.chain_id != expected {
            return Err(SiweError::ChainMismatch {
                expected,
                got: message.chain_id,
            });
        }
    }

    let now = time::OffsetDateTime::now_utc();

    if let Some(ref exp) = message.expiration_time {
        if exp < &now {
            return Err(SiweError::Expired);
        }
    }

    if let Some(ref nbf) = message.not_before {
        if nbf > &now {
            return Err(SiweError::NotYetValid);
        }
    }

    message
        .verify_eip191(signature)
        .map_err(|e| SiweError::Verification(format!("{e}")))?;

    if !nonce_store.consume(&message.nonce) {
        return Err(SiweError::NonceInvalid);
    }

    Ok(Address::from_slice(&message.address))
}

// ---------------------------------------------------------------------------
// JWT
// ---------------------------------------------------------------------------

const JWT_ISSUER: &str = "darkpool";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub iat: u64,
    pub exp: u64,
}

#[derive(Clone)]
pub struct JwtManager {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
    validation: Arc<Validation>,
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
        let mut validation = Validation::default();
        validation.set_issuer(&[JWT_ISSUER]);
        validation.set_audience(&[JWT_ISSUER]);
        Self {
            encoding: Arc::new(EncodingKey::from_secret(secret.as_bytes())),
            decoding: Arc::new(DecodingKey::from_secret(secret.as_bytes())),
            validation: Arc::new(validation),
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
            iss: JWT_ISSUER.to_string(),
            aud: JWT_ISSUER.to_string(),
            iat: now,
            exp,
        };
        let token = jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)?;
        Ok((token, exp))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let data = jsonwebtoken::decode::<Claims>(token, &self.decoding, &self.validation)?;
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
    pub expected_domain: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_generate_and_consume() {
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        assert!(store.consume(&nonce));
    }

    #[test]
    fn nonce_double_consume_fails() {
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        assert!(store.consume(&nonce));
        assert!(!store.consume(&nonce));
    }

    #[test]
    fn nonce_expired_fails() {
        let store = NonceStore::new(Duration::from_secs(0));
        let nonce = store.generate().unwrap();
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
    fn nonce_rejects_when_full() {
        let store = NonceStore::new(Duration::from_secs(300));
        for _ in 0..MAX_NONCES {
            assert!(store.generate().is_some());
        }
        assert!(store.generate().is_none());
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
        let mgr1 = JwtManager::new(
            "secret-one-xxxxxxxxxxxxxxxxxxxxxxxx",
            Duration::from_secs(3600),
        );
        let mgr2 = JwtManager::new(
            "secret-two-xxxxxxxxxxxxxxxxxxxxxxxx",
            Duration::from_secs(3600),
        );
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
            iss: JWT_ISSUER.into(),
            aud: JWT_ISSUER.into(),
            iat: 0,
            exp: 0,
        };
        let addr = JwtManager::address_from_claims(&claims).unwrap();
        assert_eq!(
            format!("{:#x}", addr),
            "0x6da01670d8fc844e736095918bbe11fe8d564163"
        );
    }

    fn sign_eip191(msg: &str, sk: &k256::ecdsa::SigningKey) -> [u8; 65] {
        use k256::ecdsa::signature::hazmat::PrehashSigner;
        use sha3::{Digest, Keccak256};
        let prefixed = format!("\x19Ethereum Signed Message:\n{}{}", msg.len(), msg);
        let hash = Keccak256::digest(prefixed.as_bytes());
        let (sig, recid): (k256::ecdsa::Signature, k256::ecdsa::RecoveryId) =
            sk.sign_prehash(hash.as_ref()).unwrap();
        let mut out = [0u8; 65];
        out[..64].copy_from_slice(&sig.to_bytes());
        out[64] = recid.to_byte() + 27;
        out
    }

    fn eth_address(sk: &k256::ecdsa::SigningKey) -> String {
        use sha3::{Digest, Keccak256};
        let pk = sk.verifying_key();
        let uncompressed = pk.to_encoded_point(false);
        let hash = Keccak256::digest(&uncompressed.as_bytes()[1..]);
        let addr_bytes = &hash[12..];
        let addr_hex = hex::encode(addr_bytes);
        let addr_hash = Keccak256::digest(addr_hex.as_bytes());
        let mut checksummed = String::with_capacity(42);
        checksummed.push_str("0x");
        for (i, c) in addr_hex.chars().enumerate() {
            if c.is_ascii_digit() {
                checksummed.push(c);
            } else if addr_hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 }) & 0xf >= 8 {
                checksummed.push(c.to_ascii_uppercase());
            } else {
                checksummed.push(c);
            }
        }
        checksummed
    }

    fn make_siwe_message(address: &str, nonce: &str, chain_id: u64) -> String {
        format!(
            "localhost wants you to sign in with your Ethereum account:\n\
             {address}\n\
             \n\
             Test statement\n\
             \n\
             URI: http://localhost\n\
             Version: 1\n\
             Chain ID: {chain_id}\n\
             Nonce: {nonce}\n\
             Issued At: 2099-01-01T00:00:00Z"
        )
    }

    #[test]
    fn verify_siwe_round_trip() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message(&addr_str, &nonce, 1);
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
        assert!(result.is_ok(), "verify failed: {result:?}");
    }

    #[test]
    fn verify_siwe_wrong_chain_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message(&addr_str, &nonce, 1);
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(42), None);
        assert!(matches!(result, Err(SiweError::ChainMismatch { .. })));
    }

    #[test]
    fn verify_siwe_wrong_domain_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message(&addr_str, &nonce, 1);
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, None, Some("evil.com"));
        assert!(matches!(result, Err(SiweError::DomainMismatch { .. })));
    }

    #[test]
    fn verify_siwe_bad_nonce_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let msg = make_siwe_message(&addr_str, "unknownnonce12345", 1);
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, None, None);
        assert!(matches!(result, Err(SiweError::NonceInvalid)));
    }

    #[test]
    fn verify_siwe_bad_signature_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message(&addr_str, &nonce, 1);
        let mut sig = sign_eip191(&msg, &sk);
        sig[0] ^= 0xff;
        let result = verify_siwe_message(&msg, &sig, &store, None, None);
        assert!(matches!(result, Err(SiweError::Verification(_))));
    }
}
