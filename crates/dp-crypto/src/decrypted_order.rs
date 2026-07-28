use alloy_primitives::Address;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use dp_types::Side;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecryptedOrder {
    pub trader: Address,
    pub pair: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub commitment_key: String,
    /// Client-chosen 32-byte blinding salt, lowercase hex (#217). The client
    /// picks it, encrypts it here, and proves the order commitment/nullifier
    /// against it; the engine reads it back to re-derive the same commitment
    /// and to key the nullifier spent-set. Carrying it inside the ciphertext —
    /// rather than deriving it server-side from a secret nonce (#153) — is what
    /// makes the commitment client-reproducible, so the per-order proof can be
    /// verified at ingestion.
    pub salt: String,
    /// Client-chosen 32-byte freshness nonce, lowercase hex (#233). The engine
    /// records `(trader, nonce)` for admitted orders, so a replay with a fresh
    /// ciphertext and fresh salt cannot reuse the same authenticated plaintext
    /// freshness token.
    pub nonce: String,
    /// Absolute order expiry in Unix nanoseconds, authenticated inside the
    /// encrypted plaintext (#233). The engine rejects already-expired orders and
    /// uses this timestamp as the resting order's expiry.
    pub expires_at_unix_nanos: i64,
    pub ttl: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_order() -> DecryptedOrder {
        DecryptedOrder {
            trader: Address::ZERO,
            pair: "ETH-USD".into(),
            side: Side::Buy,
            price: Decimal::new(250000, 2),
            size: Decimal::new(10, 1),
            commitment_key: "abc123".into(),
            salt: "11".repeat(32),
            nonce: "22".repeat(32),
            expires_at_unix_nanos: 4_102_444_800_000_000_000,
            ttl: 5_000_000_000,
        }
    }

    #[test]
    fn serde_roundtrip() {
        let o = sample_order();
        let json = serde_json::to_string(&o).unwrap();
        let back: DecryptedOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(o, back);
    }
}
