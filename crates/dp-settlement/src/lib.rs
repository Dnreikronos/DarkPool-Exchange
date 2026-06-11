mod abi;
mod eth_submitter;
mod helpers;
pub mod signer;
mod submitter;
mod watcher;

pub use abi::DarkPool;
pub use eth_submitter::{EthSubmitter, EthSubmitterConfig};
pub use helpers::{bytes32_to_uuid, decimal_to_wei, uuid_to_bytes32};
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
    /// IVC initial state: [state_hash, round_nonce, policy_hash, settlement_acc,
    /// admit_chain]. `settlement_acc` (index 3) is the #209 binding.
    pub z_0: [alloy_primitives::U256; 5],
    /// IVC final state, same 5-element layout. `z_n[3]` is the settlement
    /// hash-chain `settleAuction` recomputes from `matches[]`.
    pub z_n: [alloy_primitives::U256; 5],
    pub n_steps: u64,
    pub policy_hash: alloy_primitives::B256,
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
    #[error(
        "settlement amount carries finer than 1e8 precision, which the on-chain binding rejects"
    )]
    SubCircuitPrecision,
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
