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
