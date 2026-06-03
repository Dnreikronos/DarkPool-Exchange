use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_primitives::Address;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// NonceStore
// ---------------------------------------------------------------------------

const MAX_NONCES: usize = 10_000;

/// Per-source-IP ceiling on outstanding nonces. Bounds a single IP's
/// share of the global [`MAX_NONCES`] pool so one peer cannot fill the
/// store and 429 every other user (issue #159). A real login consumes a
/// handful of nonces, so 128 is generous headroom.
const MAX_NONCES_PER_IP: usize = 128;

#[derive(Debug)]
struct NonceEntry {
    created: Instant,
    /// IP key (see [`crate::ratelimit::ip_client_key`]) that issued this
    /// nonce, so `consume`/`evict_stale` can credit the slot back.
    owner: String,
}

#[derive(Debug, Default)]
struct NonceInner {
    nonces: HashMap<String, NonceEntry>,
    per_ip: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct NonceStore {
    inner: Arc<Mutex<NonceInner>>,
    ttl: Duration,
}

impl NonceStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(NonceInner::default())),
            ttl,
        }
    }

    /// Issue a nonce attributed to `client_key` (the caller's IP). Returns
    /// `None` if the global store is full or this key already holds
    /// `MAX_NONCES_PER_IP` outstanding nonces.
    pub fn generate_for(&self, client_key: &str) -> Option<String> {
        let mut inner = self.inner.lock();
        if inner.nonces.len() >= MAX_NONCES {
            return None;
        }
        if inner.per_ip.get(client_key).copied().unwrap_or(0) >= MAX_NONCES_PER_IP {
            return None;
        }
        let nonce = siwe::generate_nonce();
        inner.nonces.insert(
            nonce.clone(),
            NonceEntry {
                created: Instant::now(),
                owner: client_key.to_string(),
            },
        );
        *inner.per_ip.entry(client_key.to_string()).or_insert(0) += 1;
        Some(nonce)
    }

    /// Issue a nonce not attributed to any IP. Convenience for callers
    /// without a resolved peer (tests); shares one `"anonymous"` per-IP
    /// budget.
    pub fn generate(&self) -> Option<String> {
        self.generate_for("anonymous")
    }

    pub fn consume(&self, nonce: &str) -> bool {
        let mut inner = self.inner.lock();
        match inner.nonces.remove(nonce) {
            Some(entry) => {
                decrement_owner(&mut inner.per_ip, &entry.owner);
                entry.created.elapsed() <= self.ttl
            }
            None => false,
        }
    }

    pub fn evict_stale(&self) {
        let ttl = self.ttl;
        let mut inner = self.inner.lock();
        let NonceInner { nonces, per_ip } = &mut *inner;
        nonces.retain(|_, entry| {
            let keep = entry.created.elapsed() <= ttl;
            if !keep {
                decrement_owner(per_ip, &entry.owner);
            }
            keep
        });
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
        self.inner.lock().nonces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().nonces.is_empty()
    }
}

/// Decrement an owner's outstanding-nonce count, dropping the entry at
/// zero so the per-IP map cannot grow unbounded across distinct peers.
fn decrement_owner(per_ip: &mut HashMap<String, usize>, owner: &str) {
    if let Some(count) = per_ip.get_mut(owner) {
        *count -= 1;
        if *count == 0 {
            per_ip.remove(owner);
        }
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
    #[error("SIWE message must set an expiration time")]
    ExpirationRequired,
    #[error("SIWE message expiration is too far in the future")]
    ExpiryTooFar,
    #[error("SIWE message not yet valid")]
    NotYetValid,
    #[error("SIWE message issued_at is in the future")]
    IssuedInFuture,
    #[error("domain mismatch: expected {expected}, got {got}")]
    DomainMismatch { expected: String, got: String },
    #[error("chain ID mismatch: expected {expected}, got {got}")]
    ChainMismatch { expected: u64, got: u64 },
    #[error("signature verification failed: {0}")]
    Verification(String),
}

/// Clock-skew tolerance when validating the client-supplied `Issued At`.
/// The authoritative freshness bound is the server nonce TTL (see
/// [`NonceStore`]); this check only rejects a grossly future `issued_at`,
/// so generous leeway avoids false rejections from honest client clock
/// drift (issue #160). Staleness is already covered by the nonce TTL, so
/// there is deliberately no lower bound here.
const ISSUED_AT_LEEWAY: time::Duration = time::Duration::seconds(120);

/// Upper bound on how far a SIWE message's `Expiration Time` may sit past
/// server time. SIWE makes expiry optional; issue #160 requires it and
/// bounds it so a captured message cannot stay valid indefinitely —
/// defense-in-depth around the single-use nonce. Sized at ~2x the nonce
/// TTL (300 s) to absorb honest client clock skew and the client's own
/// expiry slack without false-rejecting real logins; the nonce remains
/// the authoritative single-use freshness bound.
const MAX_EXPIRY_WINDOW: time::Duration = time::Duration::seconds(600);

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

    if message.issued_at > now + ISSUED_AT_LEEWAY {
        return Err(SiweError::IssuedInFuture);
    }

    // SIWE makes `Expiration Time` optional; issue #160 requires it and
    // bounds it so a captured message cannot stay valid indefinitely.
    match message.expiration_time {
        None => return Err(SiweError::ExpirationRequired),
        Some(ref exp) => {
            if exp < &now {
                return Err(SiweError::Expired);
            }
            if exp > &(now + MAX_EXPIRY_WINDOW) {
                return Err(SiweError::ExpiryTooFar);
            }
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
        // Pin the algorithm explicitly so a token cannot dictate it
        // (defends against alg-confusion / `alg: none`). This matches the
        // jsonwebtoken default but is stated for clarity (issue #160).
        let mut validation = Validation::new(Algorithm::HS256);
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
    fn nonce_rejects_when_ip_cap_reached() {
        let store = NonceStore::new(Duration::from_secs(300));
        for _ in 0..MAX_NONCES_PER_IP {
            assert!(store.generate_for("1.2.3.4").is_some());
        }
        // The capped IP is shut out...
        assert!(store.generate_for("1.2.3.4").is_none());
        // ...but a different IP still gets served.
        assert!(store.generate_for("5.6.7.8").is_some());
    }

    #[test]
    fn nonce_rejects_when_global_cap_reached() {
        let store = NonceStore::new(Duration::from_secs(300));
        // Spread issuance across distinct keys so the per-IP cap never
        // trips before the global one.
        let mut issued = 0usize;
        'fill: for ip in 0..1024u32 {
            let key = format!("10.0.{}.{}", ip / 256, ip % 256);
            for _ in 0..MAX_NONCES_PER_IP {
                if store.generate_for(&key).is_none() {
                    break 'fill;
                }
                issued += 1;
            }
        }
        assert_eq!(issued, MAX_NONCES);
        assert!(store.generate_for("fresh-ip").is_none());
    }

    #[test]
    fn nonce_consume_frees_ip_budget() {
        let store = NonceStore::new(Duration::from_secs(300));
        let mut nonces = Vec::new();
        for _ in 0..MAX_NONCES_PER_IP {
            nonces.push(store.generate_for("1.2.3.4").unwrap());
        }
        assert!(store.generate_for("1.2.3.4").is_none());
        // Consuming one returns a slot to that IP's budget.
        assert!(store.consume(&nonces[0]));
        assert!(store.generate_for("1.2.3.4").is_some());
    }

    #[test]
    fn nonce_evict_frees_ip_budget() {
        let store = NonceStore::new(Duration::from_secs(0));
        for _ in 0..MAX_NONCES_PER_IP {
            assert!(store.generate_for("1.2.3.4").is_some());
        }
        std::thread::sleep(Duration::from_millis(10));
        store.evict_stale();
        assert_eq!(store.len(), 0);
        // Eviction credited the slots back, so the IP can issue again.
        assert!(store.generate_for("1.2.3.4").is_some());
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
        // A past `Issued At` is fine: freshness is bound by the server
        // nonce TTL, not this field. A *future* one is rejected — see
        // `verify_siwe_future_issued_at_rejected`.
        make_siwe_message_at(address, nonce, chain_id, "2024-01-01T00:00:00Z")
    }

    fn make_siwe_message_at(address: &str, nonce: &str, chain_id: u64, issued_at: &str) -> String {
        // Expiry is mandatory (issue #160); default to 5 min out, well
        // inside `MAX_EXPIRY_WINDOW`, so the happy-path builders verify.
        let exp = rfc3339_from_now(time::Duration::seconds(300));
        make_siwe_message_full(address, nonce, chain_id, issued_at, Some(&exp))
    }

    fn make_siwe_message_full(
        address: &str,
        nonce: &str,
        chain_id: u64,
        issued_at: &str,
        expiration_time: Option<&str>,
    ) -> String {
        let mut msg = format!(
            "localhost wants you to sign in with your Ethereum account:\n\
             {address}\n\
             \n\
             Test statement\n\
             \n\
             URI: http://localhost\n\
             Version: 1\n\
             Chain ID: {chain_id}\n\
             Nonce: {nonce}\n\
             Issued At: {issued_at}"
        );
        if let Some(exp) = expiration_time {
            msg.push_str(&format!("\nExpiration Time: {exp}"));
        }
        msg
    }

    fn rfc3339_from_now(offset: time::Duration) -> String {
        use time::format_description::well_known::Rfc3339;
        (time::OffsetDateTime::now_utc() + offset)
            .format(&Rfc3339)
            .unwrap()
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
    fn verify_siwe_future_issued_at_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message_at(&addr_str, &nonce, 1, "2099-01-01T00:00:00Z");
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
        assert!(matches!(result, Err(SiweError::IssuedInFuture)));
        // The nonce must survive a rejected message so an honest retry works.
        assert!(store.consume(&nonce));
    }

    /// Pin the boundary of `ISSUED_AT_LEEWAY` (120 s): an `issued_at`
    /// just inside the window is accepted, one well past it is rejected.
    /// This locks the specific tolerance rather than only proving that a
    /// year-2099 timestamp is bad.
    #[test]
    fn verify_siwe_issued_at_leeway_boundary() {
        use time::format_description::well_known::Rfc3339;

        let now = time::OffsetDateTime::now_utc();
        let at = |offset: time::Duration| (now + offset).format(&Rfc3339).unwrap();

        // 60 s into the future is within the 120 s leeway → accepted.
        {
            let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
            let addr_str = eth_address(&sk);
            let store = NonceStore::new(Duration::from_secs(300));
            let nonce = store.generate().unwrap();
            let msg = make_siwe_message_at(&addr_str, &nonce, 1, &at(time::Duration::seconds(60)));
            let sig = sign_eip191(&msg, &sk);
            let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
            assert!(result.is_ok(), "within-leeway message rejected: {result:?}");
        }

        // 200 s into the future is beyond the leeway → rejected.
        {
            let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
            let addr_str = eth_address(&sk);
            let store = NonceStore::new(Duration::from_secs(300));
            let nonce = store.generate().unwrap();
            let msg = make_siwe_message_at(&addr_str, &nonce, 1, &at(time::Duration::seconds(200)));
            let sig = sign_eip191(&msg, &sk);
            let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
            assert!(matches!(result, Err(SiweError::IssuedInFuture)));
        }
    }

    #[test]
    fn verify_siwe_missing_expiry_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let msg = make_siwe_message_full(&addr_str, &nonce, 1, "2024-01-01T00:00:00Z", None);
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
        assert!(matches!(result, Err(SiweError::ExpirationRequired)));
        // A rejected message must not burn the nonce — an honest retry works.
        assert!(store.consume(&nonce));
    }

    #[test]
    fn verify_siwe_expiry_too_far_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        // Well past the cap so this stays beyond it even with clock skew.
        let exp = rfc3339_from_now(MAX_EXPIRY_WINDOW + time::Duration::seconds(600));
        let msg = make_siwe_message_full(&addr_str, &nonce, 1, "2024-01-01T00:00:00Z", Some(&exp));
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
        assert!(matches!(result, Err(SiweError::ExpiryTooFar)));
        assert!(store.consume(&nonce));
    }

    #[test]
    fn verify_siwe_expired_rejected() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let addr_str = eth_address(&sk);
        let store = NonceStore::new(Duration::from_secs(300));
        let nonce = store.generate().unwrap();
        let exp = rfc3339_from_now(time::Duration::seconds(-60));
        let msg = make_siwe_message_full(&addr_str, &nonce, 1, "2024-01-01T00:00:00Z", Some(&exp));
        let sig = sign_eip191(&msg, &sk);
        let result = verify_siwe_message(&msg, &sig, &store, Some(1), None);
        assert!(matches!(result, Err(SiweError::Expired)));
        assert!(store.consume(&nonce));
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
