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
    #[error("pair not registered: {0}")]
    PairNotRegistered(String),
    /// Returned when an operation can't proceed because the pair is in
    /// a non-accepting state — currently `Suspended` or `Delisted`. The
    /// variant name is status-agnostic so that future statuses
    /// (e.g. `Frozen`) reuse it without another rename.
    #[error("pair not accepting orders (suspended or delisted): {0}")]
    PairNotAccepting(String),
    #[error("order size below minimum for {pair} (min {min})")]
    OrderSizeBelowMinimum { pair: String, min: String },
    #[error("price not on tick for {pair} (tick {tick})")]
    OrderPriceNotOnTick { pair: String, tick: String },
    #[error("invalid pair: {0}")]
    InvalidPair(String),
    #[error("pair already registered: {0}")]
    PairAlreadyRegistered(String),
    #[error("cannot suspend delisted pair: {0}")]
    CannotSuspendDelistedPair(String),
    #[error("trader address mismatch: expected {expected}, found {found}")]
    TraderAddressMismatch { expected: String, found: String },
    /// The same encrypted order payload was submitted more than once. ECIES is
    /// randomized, so a byte-identical ciphertext is a replay of a captured
    /// submission, not a fresh order (#233).
    #[error("duplicate order: this encrypted payload was already submitted")]
    DuplicateOrder,
    /// The decrypted order's `salt` is not a 32-byte lowercase-hex string. The
    /// client must send a well-formed blinding salt — the engine re-derives the
    /// commitment and nullifier from it (#217), so a malformed salt is a
    /// malformed order, not an internal fault.
    #[error("invalid salt: expected 32-byte lowercase hex")]
    SaltInvalid,
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
