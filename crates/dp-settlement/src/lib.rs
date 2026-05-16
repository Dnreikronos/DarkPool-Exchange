mod abi;
mod eth_submitter;
mod helpers;
mod submitter;
mod watcher;

pub use abi::DarkPool;
pub use eth_submitter::{EthSubmitter, EthSubmitterConfig};
pub use helpers::{bytes32_to_uuid, decimal_to_wei, uuid_to_bytes32};
pub use submitter::{NoopSubmitter, Submitter};
pub use watcher::{BatchSink, Watcher};

use alloy_primitives::{Address, U256};
use rust_decimal::Decimal;
use std::io;
use uuid::Uuid;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SettlementMatch {
    pub bid_order_id: Uuid,
    pub ask_order_id: Uuid,
    pub bid_trader: Address,
    pub ask_trader: Address,
    pub base_token: Address,
    pub quote_token: Address,
    pub price: Decimal,
    pub size: Decimal,
}

#[derive(Clone, Debug)]
pub struct SubmitBatchParams {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    pub proof: Vec<u8>,
    pub public_inputs: [U256; 6],
    pub matches: Vec<SettlementMatch>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettlementError {
    #[error("too many matches: {count} exceeds max {}", abi::MAX_MATCHES_PER_BATCH)]
    TooManyMatches { count: usize },
    #[error("negative amount")]
    NegativeAmount,
    #[error("precision loss converting to wei")]
    PrecisionLoss,
    #[error("invalid bytes32: high 16 bytes are non-zero")]
    InvalidBytes32,
    #[error("parse error: {0}")]
    Parse(String),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("signer error: {0}")]
    Signer(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}
