//! Zero-knowledge batch-proof primitives for DarkPool.
//!
//! Layout:
//! - [`circuit`]: Groth16 circuit over BN254 proving validity of a batch of
//!   matched order pairs.
//! - [`encoding`]: deterministic Decimal ↔ scalar conversion.
//! - [`witness`]: serializable witness types shared with `dp-zk-cli`.
//! - [`keys`]: ark-serialize wrappers over proving/verifying keys + metadata.
//! - [`pedersen`]: native + in-circuit Poseidon commitment helpers (the
//!   `pedersen` name is kept for spec parity; implementation is Poseidon).
//!
//! Two SHA256 commitments are used in DarkPool and they have different roles:
//! - `dp_crypto::compute_commitment` — binds plaintext orders to event-log
//!   placement (off-chain integrity).
//! - This crate's Poseidon commitment — binds order witness inside the ZK
//!   circuit.

pub mod circuit;
pub mod encoding;
pub mod keys;
pub mod pedersen;
pub mod witness;

pub use circuit::{prove, verify, BatchProofCircuit, ProofBytes};
pub use encoding::{decimal_to_scalar, scalar_to_decimal, EncodingError};
pub use keys::{KeyMetadata, ProvingKeyBytes, VerifyingKeyBytes};
pub use pedersen::{commit_native, OrderCommitmentInput};
pub use witness::{BatchWitness, MatchWitness, OrderLegWitness, Policy, DEFAULT_POLICY};

#[derive(Debug, thiserror::Error)]
pub enum ZkError {
    #[error("encoding: {0}")]
    Encoding(#[from] encoding::EncodingError),
    #[error("setup failed: {0}")]
    Setup(String),
    #[error("proving failed: {0}")]
    Prove(String),
    #[error("verification failed")]
    Verify,
    #[error("serialization: {0}")]
    Serialize(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid witness: {0}")]
    Witness(String),
    #[error("circuit version mismatch: keys={keys}, current={current}")]
    VersionMismatch { keys: String, current: String },
}

/// Bumped any time `BatchProofCircuit` constraints change. Used to detect
/// stale proving/verifying keys.
pub const CIRCUIT_VERSION: &str = "v2";
