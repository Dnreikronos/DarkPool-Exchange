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
        }
    }
}
