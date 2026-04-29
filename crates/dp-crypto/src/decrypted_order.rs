use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use dp_types::Side;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecryptedOrder {
    pub pair: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub commitment_key: String,
    pub ttl: i64,
}

pub fn canonical_bytes(o: &DecryptedOrder) -> Vec<u8> {
    format!(
        "pair={}|side={}|price={}|size={}|key={}|ttl={}",
        o.pair, o.side, o.price, o.size, o.commitment_key, o.ttl,
    )
    .into_bytes()
}

pub fn compute_commitment(o: &DecryptedOrder) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(canonical_bytes(o));
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_order() -> DecryptedOrder {
        DecryptedOrder {
            pair: "ETH-USD".into(),
            side: Side::Buy,
            price: Decimal::new(250000, 2),
            size: Decimal::new(10, 1),
            commitment_key: "abc123".into(),
            ttl: 5_000_000_000,
        }
    }

    #[test]
    fn canonical_bytes_format() {
        let o = sample_order();
        let s = String::from_utf8(canonical_bytes(&o)).unwrap();
        assert_eq!(s, "pair=ETH-USD|side=BUY|price=2500.00|size=1.0|key=abc123|ttl=5000000000");
    }

    #[test]
    fn canonical_bytes_sell_side() {
        let mut o = sample_order();
        o.side = Side::Sell;
        let s = String::from_utf8(canonical_bytes(&o)).unwrap();
        assert!(s.contains("side=SELL"));
    }

    #[test]
    fn commitment_determinism() {
        let o = sample_order();
        let c1 = compute_commitment(&o);
        let c2 = compute_commitment(&o);
        assert_eq!(c1.len(), 32);
        assert_eq!(c1, c2);
    }

    #[test]
    fn commitment_changes_with_input() {
        let o1 = sample_order();
        let mut o2 = sample_order();
        o2.price = Decimal::new(300000, 2);
        assert_ne!(compute_commitment(&o1), compute_commitment(&o2));
    }

    #[test]
    fn serde_roundtrip() {
        let o = sample_order();
        let json = serde_json::to_string(&o).unwrap();
        let back: DecryptedOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(o, back);
    }
}
