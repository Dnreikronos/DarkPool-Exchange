use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum DarkPoolError {
    #[error("pair is required")]
    PairRequired,
    #[error("price must be positive")]
    PriceMustBePositive,
    #[error("size must be positive")]
    SizeMustBePositive,
    #[error("commitment key is required")]
    CommitmentKeyRequired,
    #[error("limit must be > 0")]
    LimitMustBePositive,
    #[error("order not found")]
    OrderNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_match_go() {
        let cases = [
            (DarkPoolError::PairRequired, "pair is required"),
            (DarkPoolError::PriceMustBePositive, "price must be positive"),
            (DarkPoolError::SizeMustBePositive, "size must be positive"),
            (
                DarkPoolError::CommitmentKeyRequired,
                "commitment key is required",
            ),
            (DarkPoolError::LimitMustBePositive, "limit must be > 0"),
            (DarkPoolError::OrderNotFound, "order not found"),
        ];
        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected);
        }
    }
}
