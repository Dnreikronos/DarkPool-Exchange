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
//! Order commitments in DarkPool are unified on Poseidon: the engine
//! derives the canonical Poseidon commitment after decryption (see
//! `dp_engine::engine::compute_poseidon_commitment`) and that same value
//! is what binds the order inside the ZK circuit. There is no separate
//! SHA256 commitment.

pub mod circuit;
pub mod encoding;
pub mod keys;
pub mod pedersen;
pub mod witness;

pub use circuit::{compute_public_inputs, prove, verify, BatchProofCircuit, ProofBytes};
pub use encoding::{decimal_to_scalar, fr_to_bytes32, EncodingError};
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
