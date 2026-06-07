use chrono::{DateTime, Utc};
use dp_types::{EventType, Fill};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    pub data: EventData,
}

/// FUTURE (orphan tombstone, deferred): `recover.rs` re-aggregates matches
/// observed before a `BatchSubmitted` was written by minting a fresh
/// `batch_id` and emitting a *new* `BatchSubmitted` on every restart. Each
/// crash-during-batch leaks a phantom batch into the log, and the original
/// orphan matches stay unreconciled across replays. The fix is a new
/// `BatchOrphaned { auction_id, batch_id }` variant written exactly once
/// per orphan auction so subsequent replays can short-circuit. Schema
/// change → bumps the on-disk event format → not in scope for this PR.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventData {
    OrderPlaced {
        order_id: Uuid,
        commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
        #[serde(default)]
        salt_nonce: Vec<u8>,
    },
    OrderCancelled {
        order_id: Uuid,
        reason: String,
    },
    OrderExpired {
        order_id: Uuid,
    },
    AuctionExecuted {
        auction_id: Uuid,
        pair: String,
        #[serde(with = "dp_types::decimal_bincode")]
        clearing_price: Decimal,
        #[serde(with = "dp_types::decimal_bincode")]
        matched_volume: Decimal,
        match_count: u32,
        timestamp: DateTime<Utc>,
    },
    OrderMatched {
        auction_id: Uuid,
        bid: Fill,
        ask: Fill,
        #[serde(with = "dp_types::decimal_bincode")]
        price: Decimal,
        #[serde(with = "dp_types::decimal_bincode")]
        size: Decimal,
    },
    BatchSubmitted {
        batch_id: Uuid,
        auction_id: Uuid,
        tx_hash: String,
        match_count: u32,
        proof: Vec<u8>,
    },
    BatchConfirmed {
        batch_id: Uuid,
        tx_hash: String,
    },
    BatchSettled {
        batch_id: Uuid,
        block_number: u64,
        tx_hash: String,
    },
    PairRegistered {
        pair: String,
        base_token: String,
        quote_token: String,
        #[serde(with = "dp_types::decimal_bincode")]
        min_order_size: Decimal,
        #[serde(with = "dp_types::decimal_bincode")]
        tick_size: Decimal,
        #[serde(default)]
        auction_interval_ms: Option<u64>,
    },
    PairSuspended {
        pair: String,
    },
    PairDelisted {
        pair: String,
    },
    /// One IVC fold step completed. Emitted by the IVC tick path once per
    /// auction round, before the finalization boundary.
    BatchFolded {
        batch_id: Uuid,
        round_index: u64,
        pair: String,
        /// Canonical Merkle root over the commitments of every order admitted
        /// to this round's auction (#157). Folded into the proof's `z[4]`
        /// chain; published here so a watcher can recompute it from the public
        /// `OrderPlaced` log and confirm the operator matched only orders that
        /// were publicly admitted. Empty for events stored before #157.
        #[serde(default)]
        input_root: Vec<u8>,
    },
}

impl EventData {
    pub fn event_type(&self) -> EventType {
        match self {
            Self::OrderPlaced { .. } => EventType::OrderPlaced,
            Self::OrderCancelled { .. } => EventType::OrderCancelled,
            Self::OrderExpired { .. } => EventType::OrderExpired,
            Self::AuctionExecuted { .. } => EventType::AuctionExecuted,
            Self::OrderMatched { .. } => EventType::OrderMatched,
            Self::BatchSubmitted { .. } => EventType::BatchSubmitted,
            Self::BatchConfirmed { .. } => EventType::BatchConfirmed,
            Self::BatchSettled { .. } => EventType::BatchSettled,
            Self::PairRegistered { .. } => EventType::PairRegistered,
            Self::PairSuspended { .. } => EventType::PairSuspended,
            Self::PairDelisted { .. } => EventType::PairDelisted,
            Self::BatchFolded { .. } => EventType::BatchFolded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(id: u128) -> Fill {
        Fill {
            order_id: Uuid::from_u128(id),
            size: Decimal::ONE,
        }
    }

    #[test]
    fn event_type_returns_matching_variant() {
        let order_id = Uuid::from_u128(1);
        let auction_id = Uuid::from_u128(2);
        let batch_id = Uuid::from_u128(3);
        let cases: Vec<(EventData, EventType)> = vec![
            (
                EventData::OrderPlaced {
                    order_id,
                    commitment: vec![1],
                    proof: vec![2],
                    ciphertext: vec![3],
                    salt_nonce: vec![4],
                },
                EventType::OrderPlaced,
            ),
            (
                EventData::OrderCancelled {
                    order_id,
                    reason: "user".into(),
                },
                EventType::OrderCancelled,
            ),
            (
                EventData::OrderExpired { order_id },
                EventType::OrderExpired,
            ),
            (
                EventData::AuctionExecuted {
                    auction_id,
                    pair: "ETH/USDC".into(),
                    clearing_price: Decimal::new(2000, 0),
                    matched_volume: Decimal::ONE,
                    match_count: 1,
                    timestamp: Utc::now(),
                },
                EventType::AuctionExecuted,
            ),
            (
                EventData::OrderMatched {
                    auction_id,
                    bid: fill(1),
                    ask: fill(2),
                    price: Decimal::new(2000, 0),
                    size: Decimal::ONE,
                },
                EventType::OrderMatched,
            ),
            (
                EventData::BatchSubmitted {
                    batch_id,
                    auction_id,
                    tx_hash: "0xabc".into(),
                    match_count: 1,
                    proof: vec![],
                },
                EventType::BatchSubmitted,
            ),
            (
                EventData::BatchConfirmed {
                    batch_id,
                    tx_hash: "0xabc".into(),
                },
                EventType::BatchConfirmed,
            ),
            (
                EventData::BatchSettled {
                    batch_id,
                    block_number: 42,
                    tx_hash: "0xabc".into(),
                },
                EventType::BatchSettled,
            ),
            (
                EventData::PairRegistered {
                    pair: "ETH/USDC".into(),
                    base_token: "0x0".into(),
                    quote_token: "0x1".into(),
                    min_order_size: Decimal::new(1, 2),
                    tick_size: Decimal::new(1, 2),
                    auction_interval_ms: Some(5000),
                },
                EventType::PairRegistered,
            ),
            (
                EventData::PairSuspended {
                    pair: "ETH/USDC".into(),
                },
                EventType::PairSuspended,
            ),
            (
                EventData::PairDelisted {
                    pair: "ETH/USDC".into(),
                },
                EventType::PairDelisted,
            ),
            (
                EventData::BatchFolded {
                    batch_id,
                    round_index: 0,
                    pair: "ETH/USDC".into(),
                    input_root: Vec::new(),
                },
                EventType::BatchFolded,
            ),
        ];
        for (data, expected) in cases {
            assert_eq!(data.event_type(), expected);
        }
    }

    #[test]
    fn event_serde_roundtrip_preserves_event_type() {
        let original = Event {
            seq: 7,
            event_type: EventType::OrderPlaced,
            timestamp: Utc::now(),
            data: EventData::OrderExpired {
                order_id: Uuid::from_u128(99),
            },
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, original.seq);
        assert_eq!(back.event_type, original.event_type);
        assert_eq!(back.data.event_type(), EventType::OrderExpired);
    }
}
