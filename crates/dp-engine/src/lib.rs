mod batch;
mod engine;
mod error;
mod order_proof;
mod recover;
mod snapshot;
mod state;
mod subscribe;
mod tick;

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

pub use engine::{Engine, MAX_ENCRYPTED_PAYLOAD_BYTES};
pub use error::EngineError;
pub use order_proof::{Groth16OrderProofVerifier, OrderProofVerifier};
pub use snapshot::{SnapshotConfig, SnapshotError};
pub use state::{PairConfig, PairStatus, PendingBatch};
pub use subscribe::AuctionNotification;

/// Drive a single snapshot capture against an external [`SnapshotStore`].
/// Exposed for the recovery benchmark in `benches/recover.rs` so it
/// can plant a snapshot without spinning up the periodic task. Not
/// part of the supported runtime API — production callers go through
/// [`Engine::run_snapshotter`].
#[doc(hidden)]
pub fn take_snapshot_for_bench(
    engine: &Engine,
    snapshot_store: &dyn dp_event::SnapshotStore,
    config: &SnapshotConfig,
    seq_hint: u64,
) -> Result<u64, SnapshotError> {
    snapshot::take_snapshot(engine, snapshot_store, config, seq_hint)
}

pub const DEFAULT_AUCTION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
pub const DEFAULT_ORDER_TTL: std::time::Duration = std::time::Duration::from_secs(600);
pub const DEFAULT_SUBMIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const DEFAULT_MIN_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);
pub const DEFAULT_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(60);
pub const DEFAULT_SUBSCRIBER_CAPACITY: usize = 64;
