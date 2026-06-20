//! Order payload that mirrors `dp_crypto::DecryptedOrder` byte-for-byte under
//! `serde_json`. The engine deserializes the ECIES plaintext with that type,
//! so any divergence here breaks submission.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl Side {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderPayload {
    pub trader: String,
    pub pair: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub commitment_key: String,
    pub salt: String,
    pub ttl: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_matches_decrypted_order() {
        let p = OrderPayload {
            trader: "0x0000000000000000000000000000000000000000".into(),
            pair: "ETH-USD".into(),
            side: Side::Buy,
            price: Decimal::new(250000, 2),
            size: Decimal::new(10, 1),
            commitment_key: "abc123".into(),
            salt: "01".repeat(32),
            ttl: 5_000_000_000,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"trader\":\"0x0000000000000000000000000000000000000000\""));
        // side must be numeric (serde_repr); price/size must be string (rust_decimal serde-with-str).
        assert!(json.contains("\"side\":0"));
        assert!(json.contains("\"price\":\"2500.00\""));
        assert!(json.contains("\"size\":\"1.0\""));
        assert!(json.contains("\"salt\":\"0101"));
        assert!(json.contains("\"ttl\":5000000000"));
    }
}
