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
    PairRegistered = 9,
    PairSuspended = 10,
    PairDelisted = 11,
    /// Emitted by the IVC tick path each time one auction round is folded
    /// into the HyperNova accumulator. Only produced when the `hypernova`
    /// feature is active on `dp-engine`, but the discriminant is reserved
    /// here so the wire format stays stable.
    BatchFolded = 12,
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
        assert_eq!(EventType::PairRegistered as u8, 9);
        assert_eq!(EventType::PairSuspended as u8, 10);
        assert_eq!(EventType::PairDelisted as u8, 11);
        assert_eq!(EventType::BatchFolded as u8, 12);
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
