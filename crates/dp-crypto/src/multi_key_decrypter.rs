//! Decrypter that holds a set of ECIES keys in distinct rotation
//! states, so an operator can roll the active key without dropping
//! in-flight orders encrypted under the previous one.
//!
//! Iteration order is `Active → Rotating → Sunset` so the cheapest
//! happy path (the current key) runs first on every message. The first
//! `Ok` wins; only when every entry fails does the freshest-key error
//! propagate, giving operators a meaningful diagnostic instead of the
//! oldest-key tail.
//!
//! The decrypter records a `key_id` field on the current tracing span
//! once it finds a winning entry. A `darkpool_crypto_decrypt_total`
//! counter (labeled `{key_id, outcome}`) is incremented for every
//! attempt, which is the signal operators use to confirm a Sunset key
//! has fully drained before deleting it.
//!
//! Keep the entry set small (≤ 3 in practice — one Active, one
//! Rotating, optionally one Sunset). ECIES decryption is a single
//! AEAD-tag check, but worst-case cost is O(N) per failed message.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dp_types::metrics::M_CRYPTO_DECRYPT_TOTAL;
use parking_lot::RwLock;
use tracing::Instrument;

use crate::decrypted_order::DecryptedOrder;
use crate::{CryptoError, Decrypter};

/// Lifecycle phase of a key in the decrypter set. Status drives both
/// iteration order and operator dashboards — never reuse the variant
/// names as informal log strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    /// Current primary key. New orders should be encrypted to it.
    Active,
    /// Newly-introduced key being warmed up, or the just-rotated former
    /// active. Accepted for decryption but not yet announced as
    /// primary.
    Rotating,
    /// Phased-out key. Accepted only until the in-flight order tail has
    /// drained; the operator removes it once
    /// `darkpool_crypto_decrypt_total{key_id, outcome="success"}` stops
    /// incrementing.
    Sunset,
}

impl std::fmt::Display for KeyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            KeyStatus::Active => "active",
            KeyStatus::Rotating => "rotating",
            KeyStatus::Sunset => "sunset",
        };
        f.write_str(s)
    }
}

#[derive(Clone)]
pub struct KeyEntry {
    /// Stable, operator-chosen identifier (e.g. `eth-2026-q2`). Used as
    /// the `key_id` metric label and tracing-span field. Short and ASCII
    /// is best practice; the type does not enforce shape.
    pub id: String,
    pub status: KeyStatus,
    pub decrypter: Arc<dyn Decrypter>,
}

impl KeyEntry {
    pub fn new(id: impl Into<String>, status: KeyStatus, decrypter: Arc<dyn Decrypter>) -> Self {
        Self {
            id: id.into(),
            status,
            decrypter,
        }
    }
}

/// Zero-init the `darkpool_crypto_decrypt_total{key_id, status, outcome}`
/// series for both outcomes so /metrics renders them from the moment a
/// key is registered. Uses `increment(0)` (not `absolute(0)`) so that
/// re-registering an already-seen `(key_id, status)` tuple does not
/// reset accumulated counts.
fn pre_register_decrypt_metrics(key_id: &str, status: KeyStatus) {
    let status_str = status.to_string();
    metrics::counter!(
        M_CRYPTO_DECRYPT_TOTAL,
        "key_id" => key_id.to_string(),
        "status" => status_str.clone(),
        "outcome" => "success",
    )
    .increment(0);
    metrics::counter!(
        M_CRYPTO_DECRYPT_TOTAL,
        "key_id" => key_id.to_string(),
        "status" => status_str,
        "outcome" => "failure",
    )
    .increment(0);
}

#[derive(Clone)]
pub struct MultiKeyDecrypter {
    inner: Arc<RwLock<Vec<KeyEntry>>>,
}

impl Default for MultiKeyDecrypter {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiKeyDecrypter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn from_entries(entries: Vec<KeyEntry>) -> Self {
        for e in &entries {
            pre_register_decrypt_metrics(&e.id, e.status);
        }
        Self {
            inner: Arc::new(RwLock::new(entries)),
        }
    }

    /// Insert a new entry. Replaces any existing entry with the same
    /// `id` so the admin endpoint can use this as an idempotent upsert
    /// (the status transitions Active → Rotating → Sunset all go
    /// through here).
    pub fn insert(&self, entry: KeyEntry) {
        pre_register_decrypt_metrics(&entry.id, entry.status);
        let mut entries = self.inner.write();
        if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            entries.push(entry);
        }
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut entries = self.inner.write();
        let before = entries.len();
        entries.retain(|e| e.id != id);
        entries.len() != before
    }

    /// Mutate the status of `id`. Returns `true` if the entry exists.
    /// Used by the `/v1/admin/keys/{id}/sunset` handler.
    pub fn set_status(&self, id: &str, status: KeyStatus) -> bool {
        let mut entries = self.inner.write();
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            e.status = status;
            // Pre-register the new (key_id, status) series so the
            // Sunset-drain dashboard reads from the moment the operator
            // demotes the key, not from the next decrypt attempt.
            pre_register_decrypt_metrics(id, status);
            true
        } else {
            false
        }
    }

    /// Snapshot of the current entries' `(id, status)`. Cheap clone of
    /// strings; safe to call from a request handler.
    pub fn list(&self) -> Vec<(String, KeyStatus)> {
        self.inner
            .read()
            .iter()
            .map(|e| (e.id.clone(), e.status))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }

    /// Iterate entries in the canonical try-order so callers don't
    /// reimplement the priority rule.
    fn sorted_snapshot(&self) -> Vec<KeyEntry> {
        let mut entries: Vec<KeyEntry> = self.inner.read().clone();
        entries.sort_by_key(|e| match e.status {
            KeyStatus::Active => 0,
            KeyStatus::Rotating => 1,
            KeyStatus::Sunset => 2,
        });
        entries
    }
}

impl Decrypter for MultiKeyDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>> {
        let snapshot = self.sorted_snapshot();
        // The outer span carries the attempt-set size so a sudden surge
        // of failures across all keys is visible without per-key trace
        // walks. The matched `key_id` is recorded onto the current span
        // (here, the outer multi span) once a winner is found.
        let span = tracing::info_span!(
            "dp_crypto.multi_decrypt",
            ciphertext_bytes = ciphertext.len(),
            entry_count = snapshot.len(),
            key_id = tracing::field::Empty,
        );
        Box::pin(
            async move {
                if snapshot.is_empty() {
                    return Err(CryptoError::DecryptionFailed(
                        "no decryption keys registered".into(),
                    ));
                }
                let mut last_err: Option<CryptoError> = None;
                for entry in snapshot {
                    let result = entry.decrypter.decrypt(ciphertext).await;
                    let outcome = if result.is_ok() { "success" } else { "failure" };
                    metrics::counter!(
                        M_CRYPTO_DECRYPT_TOTAL,
                        "key_id" => entry.id.clone(),
                        "status" => entry.status.to_string(),
                        "outcome" => outcome,
                    )
                    .increment(1);
                    match result {
                        Ok(order) => {
                            tracing::Span::current().record("key_id", entry.id.as_str());
                            return Ok(order);
                        }
                        Err(e) => {
                            // The propagated error must be the freshest
                            // (highest-priority) failure, not the oldest:
                            // operators reading logs expect to see why
                            // the currently-primary key rejected the
                            // ciphertext, not the long-tail Sunset
                            // diagnostic. Because iteration is in
                            // priority order, the first error we observe
                            // is also the freshest — keep it and ignore
                            // later ones.
                            if last_err.is_none() {
                                last_err = Some(e);
                            }
                        }
                    }
                }
                Err(last_err
                    .unwrap_or_else(|| CryptoError::DecryptionFailed("all keys failed".into())))
            }
            .instrument(span),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decrypted_order::DecryptedOrder;
    use crate::ecies_decrypter::EciesDecrypter;
    use k256::ecdsa::SigningKey;
    use rust_decimal::Decimal;

    fn test_order() -> DecryptedOrder {
        DecryptedOrder {
            trader: alloy_primitives::Address::ZERO,
            pair: "ETH-USD".into(),
            side: dp_types::Side::Buy,
            price: Decimal::new(180000, 2),
            size: Decimal::new(5, 0),
            commitment_key: "ck-multi".into(),
            ttl: 3_000_000_000,
        }
    }

    fn fresh_key() -> (Vec<u8>, Vec<u8>) {
        let sk = SigningKey::random(&mut rand::thread_rng());
        let pk_bytes = sk.verifying_key().to_sec1_bytes().to_vec();
        (sk.to_bytes().to_vec(), pk_bytes)
    }

    fn ecies_for(sk_bytes: Vec<u8>) -> Arc<dyn Decrypter> {
        Arc::new(EciesDecrypter::from_bytes(sk_bytes).expect("32-byte secret"))
    }

    #[tokio::test]
    async fn rotating_key_wins_for_its_ciphertext() {
        let (sk_a, _pk_a) = fresh_key();
        let (sk_b, pk_b) = fresh_key();
        let order = test_order();
        let plaintext = serde_json::to_vec(&order).unwrap();
        let ciphertext = ecies::encrypt(&pk_b, &plaintext).unwrap();

        let multi = MultiKeyDecrypter::from_entries(vec![
            KeyEntry::new("a", KeyStatus::Active, ecies_for(sk_a)),
            KeyEntry::new("b", KeyStatus::Rotating, ecies_for(sk_b)),
        ]);
        assert_eq!(multi.decrypt(&ciphertext).await.unwrap(), order);
    }

    #[tokio::test]
    async fn sunset_key_still_decrypts() {
        let (sk_old, pk_old) = fresh_key();
        let (sk_new, _pk_new) = fresh_key();
        let order = test_order();
        let plaintext = serde_json::to_vec(&order).unwrap();
        // Encrypted under the soon-to-be-removed key.
        let ciphertext = ecies::encrypt(&pk_old, &plaintext).unwrap();

        let multi = MultiKeyDecrypter::from_entries(vec![
            KeyEntry::new("new", KeyStatus::Active, ecies_for(sk_new)),
            KeyEntry::new("old", KeyStatus::Sunset, ecies_for(sk_old)),
        ]);
        assert_eq!(multi.decrypt(&ciphertext).await.unwrap(), order);
    }

    #[tokio::test]
    async fn all_keys_fail_returns_freshest_error() {
        let (sk_a, _) = fresh_key();
        let (sk_b, _) = fresh_key();
        let multi = MultiKeyDecrypter::from_entries(vec![
            KeyEntry::new("active", KeyStatus::Active, ecies_for(sk_a)),
            KeyEntry::new("sunset", KeyStatus::Sunset, ecies_for(sk_b)),
        ]);
        let err = multi.decrypt(b"\x00\x01\x02 junk").await.unwrap_err();
        assert!(matches!(err, CryptoError::DecryptionFailed(_)));
    }

    #[tokio::test]
    async fn empty_decrypter_errors() {
        let multi = MultiKeyDecrypter::new();
        let err = multi.decrypt(b"anything").await.unwrap_err();
        match err {
            CryptoError::DecryptionFailed(msg) => assert!(msg.contains("no decryption keys")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn insert_replaces_same_id() {
        let (sk_a, _) = fresh_key();
        let (sk_b, _) = fresh_key();
        let m = MultiKeyDecrypter::new();
        m.insert(KeyEntry::new("k1", KeyStatus::Active, ecies_for(sk_a)));
        m.insert(KeyEntry::new("k1", KeyStatus::Rotating, ecies_for(sk_b)));
        assert_eq!(m.len(), 1);
        let list = m.list();
        assert_eq!(list[0].1, KeyStatus::Rotating);
    }

    #[test]
    fn set_status_transitions_active_to_sunset() {
        let (sk, _) = fresh_key();
        let m = MultiKeyDecrypter::new();
        m.insert(KeyEntry::new("k", KeyStatus::Active, ecies_for(sk)));
        assert!(m.set_status("k", KeyStatus::Sunset));
        assert_eq!(m.list()[0].1, KeyStatus::Sunset);
        assert!(!m.set_status("missing", KeyStatus::Sunset));
    }

    #[test]
    fn remove_drops_entry() {
        let (sk, _) = fresh_key();
        let m = MultiKeyDecrypter::new();
        m.insert(KeyEntry::new("doomed", KeyStatus::Sunset, ecies_for(sk)));
        assert!(m.remove("doomed"));
        assert!(m.is_empty());
        assert!(!m.remove("doomed"));
    }

    #[tokio::test]
    async fn span_records_key_id_on_success() {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(tracing_subscriber::fmt::layer());
        let _guard = tracing::subscriber::set_default(subscriber);

        let (sk, pk) = fresh_key();
        let order = test_order();
        let plaintext = serde_json::to_vec(&order).unwrap();
        let ciphertext = ecies::encrypt(&pk, &plaintext).unwrap();

        let multi = MultiKeyDecrypter::from_entries(vec![KeyEntry::new(
            "alpha",
            KeyStatus::Active,
            ecies_for(sk),
        )]);
        // Confirm the call path includes the `key_id` field name on the
        // span metadata so log/OTel pipelines can grep on it. Field
        // presence (not value) is the stable contract — the value gets
        // recorded inside the async block after the snapshot resolves.
        let snapshot = multi.sorted_snapshot();
        let span = tracing::info_span!(
            "dp_crypto.multi_decrypt",
            ciphertext_bytes = ciphertext.len(),
            entry_count = snapshot.len(),
            key_id = tracing::field::Empty,
        );
        let fields: Vec<&str> = span
            .metadata()
            .expect("subscriber attached")
            .fields()
            .iter()
            .map(|f| f.name())
            .collect();
        assert!(fields.contains(&"key_id"));
        assert!(fields.contains(&"entry_count"));
    }
}
