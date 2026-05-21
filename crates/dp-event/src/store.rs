use chrono::{DateTime, Utc};

use crate::{Event, EventError};

pub trait Store: Send + Sync {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError>;
    fn read_from(&self, after_seq: u64, limit: usize) -> Result<Vec<Event>, EventError>;
    fn last_seq(&self) -> u64;

    /// Liveness check for the backing storage. Default is a no-op so
    /// existing backends keep compiling; overridden by backends that
    /// can fail at runtime (network, file handle).
    ///
    /// **Runtime requirement.** Backends that bridge to async via
    /// `tokio::task::block_in_place` (notably [`crate::PgStore`]) MUST be
    /// called from a multi-threaded tokio runtime. The default
    /// `#[tokio::main]` is multi-thread so production is safe, but
    /// `#[tokio::test]` (which uses the current-thread runtime unless
    /// `flavor = "multi_thread"` is set) will panic.
    fn ping(&self) -> Result<(), EventError> {
        Ok(())
    }

    /// Approximate on-disk / in-table size of the event log in bytes.
    /// Surfaced to operators via the `darkpool_event_log_size_bytes`
    /// gauge. Returns `Ok(0)` for backends with no meaningful answer.
    ///
    /// Same runtime requirement as [`Store::ping`].
    fn size_bytes(&self) -> Result<u64, EventError> {
        Ok(0)
    }

    /// Discard every event with `seq < before_seq`. Callers MUST only
    /// invoke this after a snapshot covering at least `before_seq` has
    /// been durably written; the truncated events are unrecoverable. The
    /// default is a no-op so backends that do not (yet) support
    /// compaction keep compiling.
    ///
    /// Same runtime requirement as [`Store::ping`].
    fn compact_before(&self, _before_seq: u64) -> Result<(), EventError> {
        Ok(())
    }
}

pub fn assign_seq_and_timestamp(event: &mut Event, seq: &mut u64) {
    *seq += 1;
    event.seq = *seq;
    if event.timestamp == DateTime::<Utc>::default() {
        event.timestamp = Utc::now();
    }
}

pub fn index_after(events: &[Event], after_seq: u64) -> usize {
    if after_seq == 0 {
        return 0;
    }
    (after_seq as usize).min(events.len())
}
