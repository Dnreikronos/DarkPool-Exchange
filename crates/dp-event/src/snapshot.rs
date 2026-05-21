use std::collections::BTreeMap;

use parking_lot::RwLock;

use crate::EventError;

/// Persistent home for engine-state snapshots. Snapshots are written
/// periodically by the engine; on boot, the most recent snapshot is
/// loaded and only events with `seq > snapshot_seq` are replayed.
///
/// Backends MUST surface corruption (checksum mismatch, partial write)
/// to the caller — engine recovery is expected to drop the bad
/// envelope and fall back to a full replay, which only works if the
/// store does not silently mask the failure.
pub trait SnapshotStore: Send + Sync {
    /// Write `envelope` keyed by `seq`. Implementations should make the
    /// publish atomic so a crash mid-write does not leave a torn
    /// snapshot visible to the next [`SnapshotStore::read_latest`] call.
    fn write(&self, seq: u64, envelope: &[u8]) -> Result<(), EventError>;

    /// Return the snapshot with the highest `seq`, or `None` if the
    /// store is empty.
    fn read_latest(&self) -> Result<Option<(u64, Vec<u8>)>, EventError>;

    /// Read the envelope stored at exactly `seq`, or `None` if that
    /// seq isn't present. Recovery walks `list_seqs` descending when
    /// the latest envelope fails to decode (corruption / partial
    /// write), trying each older snapshot before giving up — without
    /// this, a single bad latest would force a full event replay even
    /// when an older, good snapshot is still on disk.
    fn read_at(&self, seq: u64) -> Result<Option<Vec<u8>>, EventError>;

    /// Sequence numbers of every snapshot the store currently holds, in
    /// ascending order. Used by the retention sweep.
    fn list_seqs(&self) -> Result<Vec<u64>, EventError>;

    /// Discard every snapshot with `seq < before_seq`. Used by the
    /// retention sweep after a fresh envelope has been written.
    fn delete_before(&self, before_seq: u64) -> Result<(), EventError>;
}

/// In-memory snapshot store. Used by tests; not durable.
#[derive(Default)]
pub struct MemSnapshotStore {
    snaps: RwLock<BTreeMap<u64, Vec<u8>>>,
}

impl MemSnapshotStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SnapshotStore for MemSnapshotStore {
    fn write(&self, seq: u64, envelope: &[u8]) -> Result<(), EventError> {
        self.snaps.write().insert(seq, envelope.to_vec());
        Ok(())
    }

    fn read_latest(&self) -> Result<Option<(u64, Vec<u8>)>, EventError> {
        Ok(self
            .snaps
            .read()
            .iter()
            .next_back()
            .map(|(s, v)| (*s, v.clone())))
    }

    fn read_at(&self, seq: u64) -> Result<Option<Vec<u8>>, EventError> {
        Ok(self.snaps.read().get(&seq).cloned())
    }

    fn list_seqs(&self) -> Result<Vec<u64>, EventError> {
        Ok(self.snaps.read().keys().copied().collect())
    }

    fn delete_before(&self, before_seq: u64) -> Result<(), EventError> {
        let mut snaps = self.snaps.write();
        let keep = snaps.split_off(&before_seq);
        *snaps = keep;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_latest_returns_highest_seq() {
        let store = MemSnapshotStore::new();
        store.write(10, b"ten").unwrap();
        store.write(20, b"twenty").unwrap();
        store.write(15, b"fifteen").unwrap();

        let (seq, env) = store.read_latest().unwrap().unwrap();
        assert_eq!(seq, 20);
        assert_eq!(env, b"twenty");
    }

    #[test]
    fn empty_read_latest_is_none() {
        let store = MemSnapshotStore::new();
        assert!(store.read_latest().unwrap().is_none());
    }

    #[test]
    fn list_seqs_ascending() {
        let store = MemSnapshotStore::new();
        store.write(30, b"x").unwrap();
        store.write(10, b"x").unwrap();
        store.write(20, b"x").unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![10, 20, 30]);
    }

    #[test]
    fn delete_before_drops_older_snapshots() {
        let store = MemSnapshotStore::new();
        store.write(1, b"a").unwrap();
        store.write(2, b"b").unwrap();
        store.write(3, b"c").unwrap();
        store.delete_before(3).unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![3]);
    }

    #[test]
    fn delete_before_zero_is_noop() {
        let store = MemSnapshotStore::new();
        store.write(5, b"x").unwrap();
        store.delete_before(0).unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![5]);
    }
}
