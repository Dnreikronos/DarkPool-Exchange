//! Client-side order construction for the DarkPool Exchange.
//!
//! Ships an ECIES-encrypted order payload, a Poseidon commitment for local
//! record-keeping, and a placeholder proof. Per-order ZK proofs are out of
//! scope — the engine runs batch-level proofs after auction matching, and
//! recomputes its own commitment server-side.

pub mod commitment;
pub mod encoding;
pub mod encrypt;
pub mod error;
pub mod payload;
pub mod submit;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use error::ClientError;
pub use payload::{OrderPayload, Side};
pub use submit::{prepare_order, prepare_order_with_entropy, OrderSubmission, PLACEHOLDER_PROOF};
