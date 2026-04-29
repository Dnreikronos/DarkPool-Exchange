use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum EventType {
    OrderPlaced = 1,
    OrderCancelled = 2,
    OrderExpired = 3,
    AuctionExecuted = 4,
    OrderMatched = 5,
    BatchSubmitted = 6,
    BatchConfirmed = 7,
    BatchSettled = 8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminant_values() {
        assert_eq!(EventType::OrderPlaced as u8, 1);
        assert_eq!(EventType::OrderCancelled as u8, 2);
        assert_eq!(EventType::OrderExpired as u8, 3);
        assert_eq!(EventType::AuctionExecuted as u8, 4);
        assert_eq!(EventType::OrderMatched as u8, 5);
        assert_eq!(EventType::BatchSubmitted as u8, 6);
        assert_eq!(EventType::BatchConfirmed as u8, 7);
        assert_eq!(EventType::BatchSettled as u8, 8);
    }

    #[test]
    fn serde_roundtrip() {
        for (variant, expected) in [
            (EventType::OrderPlaced, "1"),
            (EventType::BatchSettled, "8"),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected);
            let back: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
