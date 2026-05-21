use std::time::{Duration, Instant};

use dp_event::{EventError, SnapshotStore};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::engine::Engine;
use crate::state::SerializableState;

/// Magic bytes that prefix every snapshot envelope. Bumping the suffix
/// (e.g. `DPS2`) reserves space for a format break later — current
/// readers reject any non-`DPS1` magic with [`SnapshotError::BadMagic`].
const MAGIC: &[u8; 4] = b"DPS1";
const ENVELOPE_VERSION: u32 = 1;
const HEADER_LEN: usize = 4 /* magic */ + 4 /* version */ + 8 /* seq */ + 4 /* blob_len */ + 32 /* sha256 */;

/// Errors raised while encoding / decoding a snapshot envelope. The
/// recover path catches all of these and falls back to a full event
/// replay — these are operational hiccups, not engine bugs.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot envelope too small: {len} bytes (need at least {HEADER_LEN})")]
    Truncated { len: usize },
    #[error("bad snapshot magic: expected DPS1")]
    BadMagic,
    #[error("unsupported snapshot envelope version: {0}")]
    UnsupportedVersion(u32),
    #[error("snapshot blob_len {declared} does not match payload {actual}")]
    LengthMismatch { declared: usize, actual: usize },
    #[error("snapshot checksum mismatch — payload corrupt or truncated")]
    ChecksumMismatch,
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    #[error(transparent)]
    Store(#[from] EventError),
}

/// Operator-tunable knobs for the periodic snapshotter. The defaults
/// favour reliability over disk savings: snapshots run on a 5-minute
/// fallback even when event throughput is low, the event log keeps a
/// 1024-event tail for forensic queries, and three snapshots are
/// retained so a corrupt latest does not strand recovery.
#[derive(Clone, Debug)]
pub struct SnapshotConfig {
    pub enabled: bool,
    /// Snapshot whenever the event count since the last snapshot crosses
    /// this many events, even if [`Self::interval`] has not elapsed.
    pub every_events: u64,
    /// Force a snapshot if this long has elapsed since the last one,
    /// even if no events have accumulated. Useful for low-throughput
    /// deploys where the event-delta trigger would never fire.
    pub interval: Duration,
    /// Compact event-log entries whose seq is at least this far behind
    /// the snapshot we just wrote. Keeping a tail buys forensics: if a
    /// snapshot turns out to be wrong, the operator can still replay
    /// from raw events for the last `retain_events` worth of activity.
    pub retain_events: u64,
    /// Number of snapshot envelopes to keep on the store. Older
    /// snapshots are pruned via [`SnapshotStore::delete_before`].
    pub retain_count: usize,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            every_events: 10_000,
            interval: Duration::from_secs(300),
            retain_events: 1024,
            retain_count: 3,
        }
    }
}

/// Encode a [`SerializableState`] + its watermark `seq` into the
/// on-disk envelope. The blob carries its own SHA-256 so the recover
/// path can drop corrupted snapshots and fall back to event replay.
pub(crate) fn encode_envelope(state: &SerializableState, seq: u64) -> Result<Vec<u8>, SnapshotError> {
    let blob = bincode::serialize(state)?;
    let blob_len = u32::try_from(blob.len()).map_err(|_| SnapshotError::LengthMismatch {
        declared: u32::MAX as usize,
        actual: blob.len(),
    })?;
    let digest = Sha256::digest(&blob);

    let mut out = Vec::with_capacity(HEADER_LEN + blob.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ENVELOPE_VERSION.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&blob_len.to_be_bytes());
    out.extend_from_slice(&digest);
    out.extend_from_slice(&blob);
    Ok(out)
}

/// Decode an envelope produced by [`encode_envelope`], verifying the
/// magic, version, declared length, and SHA-256 checksum.
pub(crate) fn decode_envelope(bytes: &[u8]) -> Result<(u64, SerializableState), SnapshotError> {
    if bytes.len() < HEADER_LEN {
        return Err(SnapshotError::Truncated { len: bytes.len() });
    }
    if &bytes[0..4] != MAGIC {
        return Err(SnapshotError::BadMagic);
    }
    let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    if version != ENVELOPE_VERSION {
        return Err(SnapshotError::UnsupportedVersion(version));
    }
    let seq = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
    let blob_len = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let checksum = &bytes[20..52];
    let payload = &bytes[HEADER_LEN..];
    if payload.len() != blob_len {
        return Err(SnapshotError::LengthMismatch {
            declared: blob_len,
            actual: payload.len(),
        });
    }
    let digest = Sha256::digest(payload);
    if &digest[..] != checksum {
        return Err(SnapshotError::ChecksumMismatch);
    }
    let state: SerializableState = bincode::deserialize(payload)?;
    Ok((seq, state))
}

/// Periodic snapshotter task. Lives next to the auction tick: every
/// `min(interval, 1s)` wake-up, the task asks the engine whether a
/// snapshot is due based on event-count delta or wall-clock elapsed,
/// captures one if so, then compacts events and old snapshots behind
/// the new watermark. Errors are logged at `warn!` — a missed snapshot
/// is recoverable, so the task never panics or aborts on its own.
pub(crate) async fn run_snapshotter(
    engine: Engine,
    config: SnapshotConfig,
    cancel: CancellationToken,
) {
    if !config.enabled {
        info!("snapshotter disabled — periodic snapshots will NOT run");
        return;
    }
    let Some(snapshot_store) = engine.snapshot_store_clone() else {
        info!("snapshotter started without a SnapshotStore — task exiting");
        return;
    };

    // Sub-tick at the smaller of (interval, 1s) so the event-delta
    // trigger can fire promptly for high-throughput deploys without
    // demanding a tight per-call latency budget.
    let sub_tick = config
        .interval
        .min(Duration::from_secs(1))
        .max(Duration::from_millis(100));
    let mut interval = tokio::time::interval(sub_tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Seed the trigger watermark from the most recent envelope on disk
    // so a restart on a populated store does not immediately re-emit a
    // redundant snapshot (the event-delta trigger would otherwise fire
    // on the very first sub-tick whenever `store.last_seq() >= every_events`).
    let mut last_snapshot_seq: u64 = match snapshot_store.read_latest() {
        Ok(Some((seq, _))) => seq,
        Ok(None) => 0,
        Err(e) => {
            warn!(error = ?e, "could not read latest snapshot seq at boot; starting from 0");
            0
        }
    };
    let mut last_snapshot_at = Instant::now();
    info!(
        every_events = config.every_events,
        interval_secs = config.interval.as_secs(),
        retain_events = config.retain_events,
        retain_count = config.retain_count,
        "snapshotter started"
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("snapshotter cancelled");
                return;
            }
            _ = interval.tick() => {
                let now_seq = engine.store_last_seq();
                let elapsed = last_snapshot_at.elapsed();
                let event_trigger = now_seq.saturating_sub(last_snapshot_seq) >= config.every_events;
                let time_trigger = elapsed >= config.interval;
                if now_seq <= last_snapshot_seq {
                    continue;
                }
                if !event_trigger && !time_trigger {
                    continue;
                }
                match take_snapshot(&engine, snapshot_store.as_ref(), &config, now_seq) {
                    Ok(written_seq) => {
                        last_snapshot_seq = written_seq;
                        last_snapshot_at = Instant::now();
                    }
                    Err(e) => warn!(error = ?e, "snapshot tick failed; will retry next tick"),
                }
            }
        }
    }
}

/// Capture, write, compact, prune. Returns the seq the snapshot was
/// keyed under so the loop can drive its trigger logic. Pulled out of
/// [`run_snapshotter`] so unit tests can drive a snapshot without a
/// running async task.
pub(crate) fn take_snapshot(
    engine: &Engine,
    snapshot_store: &dyn SnapshotStore,
    config: &SnapshotConfig,
    seq_hint: u64,
) -> Result<u64, SnapshotError> {
    // Capture under lock — clone the persistable subset and pick a seq
    // *while holding the lock* so the snapshot reflects exactly the
    // event the engine had observed at capture time.
    let (state, seq) = engine.capture_snapshot_state(seq_hint);

    let envelope = encode_envelope(&state, seq)?;
    snapshot_store.write(seq, &envelope)?;

    // Compact the event log behind the new watermark. `retain_events`
    // is the tail kept for forensics; underflow is fine because
    // `compact_before(0)` is a no-op in every backend.
    let compact_floor = seq.saturating_sub(config.retain_events);
    if compact_floor > 0 {
        if let Err(e) = engine.compact_events_before(compact_floor) {
            warn!(error = ?e, seq, "event-log compaction failed (snapshot already written)");
        }
    }

    // Retention sweep on the snapshot store. Keep the latest
    // `retain_count` envelopes including the one we just wrote.
    if config.retain_count > 0 {
        if let Ok(seqs) = snapshot_store.list_seqs() {
            if seqs.len() > config.retain_count {
                let cutoff_idx = seqs.len() - config.retain_count;
                let cutoff_seq = seqs[cutoff_idx];
                if let Err(e) = snapshot_store.delete_before(cutoff_seq) {
                    warn!(error = ?e, "snapshot retention prune failed");
                }
            }
        }
    }
    info!(seq, "snapshot written");
    Ok(seq)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::state::SerializableState;
    use dp_book::OrderBook;

    fn empty_state() -> SerializableState {
        SerializableState {
            book: OrderBook::new().to_snapshot(),
            pair_tokens: HashMap::new(),
            auction_log: Vec::new(),
            pending_batches: HashMap::new(),
        }
    }

    #[test]
    fn envelope_round_trip() {
        let s = empty_state();
        let env = encode_envelope(&s, 1234).unwrap();
        let (seq, _back) = decode_envelope(&env).unwrap();
        assert_eq!(seq, 1234);
    }

    #[test]
    fn detects_truncation() {
        let env = encode_envelope(&empty_state(), 1).unwrap();
        let half = &env[..env.len() / 2];
        let err = decode_envelope(half).unwrap_err();
        assert!(
            matches!(
                err,
                SnapshotError::LengthMismatch { .. } | SnapshotError::Truncated { .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn detects_bad_magic() {
        let mut env = encode_envelope(&empty_state(), 1).unwrap();
        env[0] = b'X';
        let err = decode_envelope(&env).unwrap_err();
        assert!(matches!(err, SnapshotError::BadMagic), "got {err:?}");
    }

    #[test]
    fn detects_corruption_in_payload() {
        let mut env = encode_envelope(&empty_state(), 1).unwrap();
        // Mid-payload flip — outside the header, so length and magic
        // still look OK; only the checksum catches it.
        let last = env.len() - 1;
        env[last] ^= 0xFF;
        let err = decode_envelope(&env).unwrap_err();
        assert!(
            matches!(err, SnapshotError::ChecksumMismatch),
            "got {err:?}"
        );
    }
}
