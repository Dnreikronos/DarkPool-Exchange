use chrono::{DateTime, Utc};

use crate::{Event, EventError};

pub trait Store: Send + Sync {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError>;
    fn read_from(&self, after_seq: u64, limit: usize) -> Result<Vec<Event>, EventError>;
    fn last_seq(&self) -> u64;
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
