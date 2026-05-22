mod abi;
mod eth_submitter;
mod helpers;
pub mod signer;
mod submitter;
mod watcher;

pub use abi::DarkPool;
pub use eth_submitter::{EthSubmitter, EthSubmitterConfig};
pub use helpers::{bytes32_to_uuid, decimal_to_wei, uuid_to_bytes32};
#[cfg(feature = "hypernova")]
pub use helpers::compute_matches_hash;
pub use signer::{LocalTxSigner, TxSigner};
pub use submitter::{NoopSubmitter, Submitter};
pub use watcher::{BatchSink, Watcher};

use alloy_primitives::{Address, U256};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::io;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug)]
pub struct SubmitSessionParams {
    pub session_id: Uuid,
    pub proof: Vec<u8>,
    pub z_0: [alloy_primitives::U256; 3],
    pub z_n: [alloy_primitives::U256; 3],
    pub n_steps: u64,
    pub policy_hash: alloy_primitives::B256,
    /// keccak256(abi.encode(auctionId, matches)) — the operator's pre-commitment
    /// to the exact matches array that `settle_auction` will replay. The on-chain
    /// `settleAuction` re-derives this and rejects any substitution.
    pub matches_hash: alloy_primitives::B256,
    pub auction_id: Uuid,
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
