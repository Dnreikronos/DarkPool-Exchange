//! Zero-knowledge batch-proof primitives for DarkPool.
//!
//! Layout:
//! - [`encoding`]: deterministic Decimal ↔ scalar conversion.
//! - [`witness`]: serializable witness types shared with `dp-zk-cli`.
//! - [`pedersen`]: native + in-circuit Poseidon commitment helpers (the
//!   `pedersen` name is kept for spec parity; implementation is Poseidon).
//! - [`step_circuit`]: HyperNova IVC step circuit.
//! - [`folding`]: IVC folding API (HyperNova via sonobe).
//! - [`params`]: HyperNova params persistence helpers.
//!
//! Order commitments in DarkPool are unified on Poseidon: the engine
//! derives the canonical Poseidon commitment after decryption (see
//! `dp_engine::engine::compute_poseidon_commitment`) and that same value
//! is what binds the order inside the ZK circuit. There is no separate
//! SHA256 commitment.

pub mod encoding;
pub mod folding;
pub mod params;
pub mod pedersen;
pub mod step_circuit;
pub mod witness;

pub use encoding::{decimal_to_scalar, fr_to_bytes32, EncodingError};
pub use folding::{
    compress_and_finalize, fold_step, generate_params, init_accumulator, verify_final, FinalProof,
    FoldingAccumulator, HyperNovaPublicParams,
};
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
    #[error("ivc error: {0}")]
    Ivc(String),
}

/// Bumped any time the IVC circuit constraints change.
pub const CIRCUIT_VERSION: &str = "v2";
