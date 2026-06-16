use std::time::{Duration, Instant};

use dp_crypto::SnapshotCipher;
use dp_event::{EventError, SnapshotStore};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::engine::Engine;
use crate::state::SerializableState;

/// Magic bytes that prefix every snapshot envelope. `DPS2` marks the
/// encrypted-at-rest format (#203): the payload is XChaCha20-Poly1305
/// ciphertext, not plaintext bincode. Readers reject any non-`DPS2` magic
/// with [`SnapshotError::BadMagic`] — a stray `DPS1` (plaintext) envelope is
/// treated as corrupt and the recover path falls back to event replay.
const MAGIC: &[u8; 4] = b"DPS2";
const ENVELOPE_VERSION: u32 = 2;
/// Bytes bound into the AEAD tag as associated data: `magic || version || seq`.
/// Binding the seq stops a sealed blob from being relabeled under another seq.
const AAD_LEN: usize = 4 /* magic */ + 4 /* version */ + 8 /* seq */;
/// Fixed envelope prefix: the [`AAD_LEN`] header plus a 4-byte sealed length.
/// The sealed payload (nonce || ciphertext+tag) follows.
const HEADER_LEN: usize = AAD_LEN + 4 /* sealed_len */;

/// Errors raised while encoding / decoding a snapshot envelope. The
/// recover path catches all of these and falls back to a full event
/// replay — these are operational hiccups, not engine bugs.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot envelope too small: {len} bytes (need at least {HEADER_LEN})")]
    Truncated { len: usize },
    #[error("bad snapshot magic: expected DPS2")]
    BadMagic,
    #[error("unsupported snapshot envelope version: {0}")]
    UnsupportedVersion(u32),
    #[error("snapshot sealed_len {declared} does not match payload {actual}")]
    LengthMismatch { declared: usize, actual: usize },
    /// AEAD open failed: the payload is corrupt, truncated, tampered, or was
    /// sealed under a different key. The recover path treats this exactly like
    /// any other corrupt envelope (try an older one, else fall back to replay).
    #[error("snapshot decryption/authentication failed — corrupt, tampered, or wrong key")]
    Decrypt,
    /// AEAD seal failed while writing — an internal cipher error, not a
    /// recoverable condition. Surfaced so the snapshotter logs and retries.
    #[error("snapshot encryption failed: {0}")]
    Encrypt(String),
    /// No [`SnapshotCipher`] was installed. Fail closed rather than write or
    /// read a plaintext snapshot. The boot path must configure a snapshot key.
    #[error("no snapshot cipher configured — refusing plaintext snapshots at rest")]
    CipherMissing,
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

/// Encode a [`SerializableState`] + its watermark `seq` into the on-disk
/// envelope. The serialized state is sealed with `cipher` (XChaCha20-Poly1305)
/// so the store never holds plaintext order data (#203). The Poly1305 tag is
/// the integrity check — a tampered payload fails [`decode_envelope`] and the
/// recover path falls back to event replay.
///
/// Layout: `magic(4) | version(4) | seq(8) | sealed_len(4) | sealed`, where
/// `sealed = nonce(24) || ciphertext+tag`. The `magic||version||seq` prefix is
/// the AEAD associated data, so neither the version nor the seq can be altered
/// without failing the tag.
pub(crate) fn encode_envelope(
    state: &SerializableState,
    seq: u64,
    cipher: &SnapshotCipher,
) -> Result<Vec<u8>, SnapshotError> {
    let blob = bincode::serialize(state)?;

    let mut header = Vec::with_capacity(AAD_LEN);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&ENVELOPE_VERSION.to_be_bytes());
    header.extend_from_slice(&seq.to_be_bytes());

    let sealed = cipher
        .seal(&blob, &header)
        .map_err(|e| SnapshotError::Encrypt(e.to_string()))?;
    let sealed_len = u32::try_from(sealed.len()).map_err(|_| SnapshotError::LengthMismatch {
        declared: u32::MAX as usize,
        actual: sealed.len(),
    })?;

    let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
    out.extend_from_slice(&header); // magic || version || seq (== AAD)
    out.extend_from_slice(&sealed_len.to_be_bytes());
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Decode an envelope produced by [`encode_envelope`], verifying the magic,
/// version, declared length, then opening the AEAD payload with `cipher`. A
/// failed open (corruption, tamper, wrong key) maps to [`SnapshotError::Decrypt`].
pub(crate) fn decode_envelope(
    bytes: &[u8],
    cipher: &SnapshotCipher,
) -> Result<(u64, SerializableState), SnapshotError> {
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
    let sealed_len = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let payload = &bytes[HEADER_LEN..];
    if payload.len() != sealed_len {
        return Err(SnapshotError::LengthMismatch {
            declared: sealed_len,
            actual: payload.len(),
        });
    }
    // AAD is the stored magic||version||seq prefix verbatim — any flip there
    // (e.g. a relabeled seq) fails the tag below rather than being trusted.
    let aad = &bytes[0..AAD_LEN];
    let blob = cipher
        .open(payload, aad)
        .map_err(|_| SnapshotError::Decrypt)?;
    let state: SerializableState = bincode::deserialize(&blob)?;
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
    // Fail closed: a store without a cipher would mean plaintext order data at
    // rest. Refuse to run rather than leak (#203). The boot path guarantees a
    // cipher whenever a durable store is configured.
    if engine.snapshot_cipher_clone().is_none() {
        warn!(
            "snapshotter started without a SnapshotCipher — refusing to write \
             plaintext snapshots; task exiting (set DARKPOOL_SNAPSHOT_KEY_URI)"
        );
        return;
    }

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
                if now_seq < last_snapshot_seq {
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
    // Fail closed: never write a snapshot without a cipher to seal it. The
    // boot path installs one whenever a store is wired; this guards against a
    // library misuse writing plaintext order data to disk.
    let cipher = engine
        .snapshot_cipher_clone()
        .ok_or(SnapshotError::CipherMissing)?;

    // Capture under lock — clone the persistable subset and pick a seq
    // *while holding the lock* so the snapshot reflects exactly the
    // event the engine had observed at capture time.
    let (state, seq) = engine.capture_snapshot_state(seq_hint);

    let envelope = encode_envelope(&state, seq, &cipher)?;
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
    use std::sync::Arc;
    use std::time::Duration;

    use dp_crypto::SnapshotCipher;
    use dp_event::{MemSnapshotStore, MemStore};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::state::{PairConfig, SerializableState};
    use dp_book::OrderBook;

    fn empty_state() -> SerializableState {
        SerializableState {
            book: OrderBook::new().to_snapshot(),
            pair_tokens: HashMap::new(),
            auction_log: Vec::new(),
            pending_batches: HashMap::new(),
            seen_ciphertexts: Default::default(),
            spent_nullifiers: Default::default(),
        }
    }

    /// Deterministic cipher for envelope tests. Real deploys load a key via
    /// `DARKPOOL_SNAPSHOT_KEY_URI`; tests fix the key so failures are stable.
    fn test_cipher() -> Arc<SnapshotCipher> {
        Arc::new(SnapshotCipher::from_bytes(&[7u8; 32]).unwrap())
    }

    fn bare_engine() -> crate::engine::Engine {
        let store = Arc::new(MemStore::new());
        let engine = crate::engine::Engine::new(store, Duration::from_millis(50));
        engine.set_snapshot_cipher(Some(test_cipher()));
        engine
    }

    #[test]
    fn envelope_round_trip() {
        let c = test_cipher();
        let s = empty_state();
        let env = encode_envelope(&s, 1234, &c).unwrap();
        let (seq, _back) = decode_envelope(&env, &c).unwrap();
        assert_eq!(seq, 1234);
    }

    #[test]
    fn envelope_hides_plaintext() {
        // A pair key is a cleartext string field in the serialized state. The
        // unencrypted bincode contains it verbatim; the sealed envelope must
        // not. The first assert keeps the second honest (proves the marker is
        // really in the plaintext form, so a passing canary means something).
        const MARKER: &str = "MARKER-SECRET-PAIR";
        let mut s = empty_state();
        s.pair_tokens
            .insert(MARKER.to_string(), PairConfig::default());

        let plain = bincode::serialize(&s).unwrap();
        assert!(
            plain.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()),
            "marker must appear in the unencrypted bincode"
        );

        let env = encode_envelope(&s, 1, &test_cipher()).unwrap();
        assert!(
            !env.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()),
            "plaintext pair key leaked into the encrypted envelope"
        );
    }

    #[test]
    fn detects_truncation() {
        let c = test_cipher();
        let env = encode_envelope(&empty_state(), 1, &c).unwrap();
        let half = &env[..env.len() / 2];
        let err = decode_envelope(half, &c).unwrap_err();
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
        let c = test_cipher();
        let mut env = encode_envelope(&empty_state(), 1, &c).unwrap();
        env[0] = b'X';
        let err = decode_envelope(&env, &c).unwrap_err();
        assert!(matches!(err, SnapshotError::BadMagic), "got {err:?}");
    }

    #[test]
    fn detects_unsupported_version() {
        let c = test_cipher();
        let mut env = encode_envelope(&empty_state(), 1, &c).unwrap();
        // Version sits at bytes 4..8 in big-endian.
        let bad_ver: u32 = 0xDEAD;
        env[4..8].copy_from_slice(&bad_ver.to_be_bytes());
        let err = decode_envelope(&env, &c).unwrap_err();
        assert!(
            matches!(err, SnapshotError::UnsupportedVersion(v) if v == bad_ver),
            "got {err:?}",
        );
    }

    #[test]
    fn detects_tampered_ciphertext() {
        let c = test_cipher();
        let mut env = encode_envelope(&empty_state(), 1, &c).unwrap();
        // Flip the last byte (inside the Poly1305 tag) — length and magic
        // still look OK, so only the AEAD open catches it.
        let last = env.len() - 1;
        env[last] ^= 0xFF;
        let err = decode_envelope(&env, &c).unwrap_err();
        assert!(matches!(err, SnapshotError::Decrypt), "got {err:?}");
    }

    #[test]
    fn take_snapshot_writes_and_returns_seq() {
        let engine = bare_engine();
        let snap_store = MemSnapshotStore::new();
        let cfg = SnapshotConfig {
            enabled: true,
            every_events: 100,
            interval: Duration::from_secs(300),
            retain_events: 0,
            retain_count: 5,
        };
        let seq = take_snapshot(&engine, &snap_store, &cfg, 0).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(snap_store.list_seqs().unwrap(), vec![0]);
    }

    #[test]
    fn take_snapshot_prunes_old_envelopes_beyond_retain_count() {
        let engine = bare_engine();
        let snap_store = MemSnapshotStore::new();
        // Pre-populate 4 old envelopes at seqs 1..4.
        for s in [1u64, 2, 3, 4] {
            let env = encode_envelope(&empty_state(), s, &test_cipher()).unwrap();
            snap_store.write(s, &env).unwrap();
        }
        let cfg = SnapshotConfig {
            retain_count: 3,
            retain_events: 0,
            ..SnapshotConfig::default()
        };
        // The bare engine has no events so take_snapshot captures seq=0.
        // After the write, the store holds seqs [0,1,2,3,4] (5 entries).
        // retain_count=3 → keep the 3 highest, delete before seqs[2]=2.
        // Result: [2, 3, 4].
        take_snapshot(&engine, &snap_store, &cfg, 0).unwrap();
        assert_eq!(snap_store.list_seqs().unwrap(), vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn run_snapshotter_writes_snapshot_on_event_trigger() {
        let engine = bare_engine();
        let snap_store = Arc::new(MemSnapshotStore::new());
        engine.set_snapshot_store(Some(snap_store.clone()));

        let cfg = SnapshotConfig {
            enabled: true,
            // Fire on every event (0 means always ≥ threshold).
            every_events: 0,
            // Long interval so only event_trigger drives the first fire.
            interval: Duration::from_secs(300),
            retain_events: 0,
            retain_count: 10,
        };

        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();
        let engine_clone = engine.clone();

        let handle = tokio::spawn(async move {
            run_snapshotter(engine_clone, cfg, cancel2).await;
        });

        // Allow up to 500 ms for at least one snapshot to land.
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        while snap_store.list_seqs().unwrap().is_empty() {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;

        assert!(
            !snap_store.list_seqs().unwrap().is_empty(),
            "run_snapshotter must write at least one snapshot"
        );
    }

    #[tokio::test]
    async fn run_snapshotter_exits_immediately_when_disabled() {
        let engine = bare_engine();
        let cfg = SnapshotConfig {
            enabled: false,
            ..SnapshotConfig::default()
        };
        let cancel = CancellationToken::new();
        // Should return promptly without hanging.
        tokio::time::timeout(
            Duration::from_millis(200),
            run_snapshotter(engine, cfg, cancel),
        )
        .await
        .expect("disabled snapshotter must return immediately");
    }

    #[tokio::test]
    async fn run_snapshotter_exits_when_no_snapshot_store() {
        let engine = bare_engine();
        // No snapshot store set — task must exit cleanly.
        let cfg = SnapshotConfig::default();
        let cancel = CancellationToken::new();
        tokio::time::timeout(
            Duration::from_millis(200),
            run_snapshotter(engine, cfg, cancel),
        )
        .await
        .expect("snapshotter without store must return immediately");
    }

    #[test]
    fn take_snapshot_without_cipher_fails_closed() {
        // An engine with a store but no cipher must refuse to write rather
        // than emit a plaintext snapshot (#203).
        let store = Arc::new(MemStore::new());
        let engine = crate::engine::Engine::new(store, Duration::from_millis(50));
        let snap_store = MemSnapshotStore::new();
        let err = take_snapshot(&engine, &snap_store, &SnapshotConfig::default(), 0).unwrap_err();
        assert!(matches!(err, SnapshotError::CipherMissing), "got {err:?}");
        assert!(
            snap_store.list_seqs().unwrap().is_empty(),
            "fail-closed must not write any envelope"
        );
    }

    #[tokio::test]
    async fn run_snapshotter_exits_without_cipher() {
        let store = Arc::new(MemStore::new());
        let engine = crate::engine::Engine::new(store, Duration::from_millis(50));
        let snap_store = Arc::new(MemSnapshotStore::new());
        engine.set_snapshot_store(Some(snap_store.clone()));
        // Store wired but no cipher → fail closed, write nothing, exit.
        let cfg = SnapshotConfig {
            every_events: 0,
            ..SnapshotConfig::default()
        };
        let cancel = CancellationToken::new();
        tokio::time::timeout(
            Duration::from_millis(200),
            run_snapshotter(engine, cfg, cancel),
        )
        .await
        .expect("snapshotter without cipher must return immediately");
        assert!(
            snap_store.list_seqs().unwrap().is_empty(),
            "fail-closed must not write plaintext"
        );
    }
}
