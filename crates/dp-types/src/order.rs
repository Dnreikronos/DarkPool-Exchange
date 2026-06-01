use alloy_primitives::Address;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Side;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Order {
    #[serde(rename = "ID")]
    pub id: Uuid,
    pub trader: Address,
    pub pair: String,
    pub side: Side,
    #[serde(with = "crate::decimal_bincode")]
    pub price: Decimal,
    #[serde(with = "crate::decimal_bincode")]
    pub size: Decimal,
    #[serde(with = "crate::decimal_bincode")]
    pub remaining_size: Decimal,
    pub commitment_key: String,
    pub encrypted_payload: Vec<u8>,
    pub submitted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Monotonic placement sequence — the event-store `seq` assigned to this
    /// order's `OrderPlaced` event. Strictly increasing across every order and
    /// identical on live placement and on event-log replay, so it is the
    /// canonical price-time priority key for matching (see ADR 0005).
    ///
    /// `submitted_at` is a wall-clock instant for display and TTL only; it must
    /// never drive matching priority because it is non-monotonic at
    /// sub-millisecond granularity and is reconstructed from the (possibly
    /// DB-write) event timestamp on recovery. `#[serde(default)]` keeps older
    /// snapshots that predate this field deserialisable (they replay to the
    /// correct `seq` from their `OrderPlaced` events).
    #[serde(default)]
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Fill {
    #[serde(rename = "OrderID")]
    pub order_id: Uuid,
    #[serde(with = "crate::decimal_bincode")]
    pub size: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_json_field_names() {
        let order = Order {
            id: Uuid::nil(),
            trader: Address::ZERO,
            pair: "BTC-USD".into(),
            side: Side::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
            remaining_size: Decimal::new(1, 0),
            commitment_key: "key".into(),
            encrypted_payload: vec![],
            submitted_at: DateTime::default(),
            expires_at: DateTime::default(),
            seq: 0,
        };
        let json = serde_json::to_value(&order).unwrap();
        assert!(json.get("ID").is_some(), "expected ID field");
        assert!(json.get("Pair").is_some(), "expected Pair field");
        assert!(json.get("Side").is_some(), "expected Side field");
        assert!(json.get("Price").is_some(), "expected Price field");
        assert!(
            json.get("RemainingSize").is_some(),
            "expected RemainingSize field"
        );
        assert!(
            json.get("CommitmentKey").is_some(),
            "expected CommitmentKey field"
        );
        assert!(
            json.get("EncryptedPayload").is_some(),
            "expected EncryptedPayload field"
        );
        assert!(
            json.get("SubmittedAt").is_some(),
            "expected SubmittedAt field"
        );
        assert!(json.get("ExpiresAt").is_some(), "expected ExpiresAt field");
        assert!(json.get("Seq").is_some(), "expected Seq field");
    }

    #[test]
    fn fill_json_field_names() {
        let fill = Fill {
            order_id: Uuid::nil(),
            size: Decimal::new(5, 1),
        };
        let json = serde_json::to_value(&fill).unwrap();
        assert!(json.get("OrderID").is_some(), "expected OrderID field");
        assert!(json.get("Size").is_some(), "expected Size field");
    }
}
