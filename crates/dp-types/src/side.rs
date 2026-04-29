use std::fmt;

use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() {
        assert_eq!(Side::Buy.to_string(), "BUY");
        assert_eq!(Side::Sell.to_string(), "SELL");
    }

    #[test]
    fn serde_roundtrip() {
        let json = serde_json::to_string(&Side::Buy).unwrap();
        assert_eq!(json, "0");
        let back: Side = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Side::Buy);

        let json = serde_json::to_string(&Side::Sell).unwrap();
        assert_eq!(json, "1");
        let back: Side = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Side::Sell);
    }
}
