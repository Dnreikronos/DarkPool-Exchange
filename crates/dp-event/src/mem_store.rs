use parking_lot::RwLock;

use crate::{assign_seq_and_timestamp, index_after, Event, EventError, Store};

struct Inner {
    events: Vec<Event>,
    seq: u64,
}

pub struct MemStore {
    inner: RwLock<Inner>,
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                events: Vec::new(),
                seq: 0,
            }),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for MemStore {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError> {
        let mut inner = self.inner.write();
        for e in events.iter_mut() {
            assign_seq_and_timestamp(e, &mut inner.seq);
            inner.events.push(e.clone());
        }
        Ok(())
    }

    fn read_from(&self, after_seq: u64, limit: usize) -> Result<Vec<Event>, EventError> {
        if limit == 0 {
            return Err(EventError::LimitMustBePositive);
        }
        let inner = self.inner.read();
        let start = index_after(&inner.events, after_seq);
        if start >= inner.events.len() {
            return Ok(Vec::new());
        }
        let end = (start + limit).min(inner.events.len());
        Ok(inner.events[start..end].to_vec())
    }

    fn last_seq(&self) -> u64 {
        self.inner.read().seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use crate::EventData;
    use dp_types::EventType;
    use uuid::Uuid;

    fn placed_event() -> Event {
        Event {
            seq: 0,
            event_type: EventType::OrderPlaced,
            timestamp: DateTime::default(),
            data: EventData::OrderPlaced {
                order_id: Uuid::new_v4(),
                commitment: vec![1, 2, 3],
                proof: vec![4, 5, 6],
                ciphertext: vec![7, 8, 9],
            },
        }
    }

    #[test]
    fn append_and_read() {
        let store = MemStore::new();
        let mut events = vec![placed_event(), placed_event()];
        store.append(&mut events).unwrap();

        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
        assert_eq!(store.last_seq(), 2);

        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 1);
        assert_eq!(read[1].seq, 2);
    }

    #[test]
    fn offset_read() {
        let store = MemStore::new();
        let mut events = vec![placed_event(), placed_event(), placed_event()];
        store.append(&mut events).unwrap();

        let read = store.read_from(1, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 2);
    }

    #[test]
    fn invalid_limit() {
        let store = MemStore::new();
        let err = store.read_from(0, 0).unwrap_err();
        assert!(matches!(err, EventError::LimitMustBePositive));
    }

    #[test]
    fn empty_store() {
        let store = MemStore::new();
        assert_eq!(store.last_seq(), 0);
        let read = store.read_from(0, 10).unwrap();
        assert!(read.is_empty());
    }

    #[test]
    fn sets_timestamp_when_default() {
        let store = MemStore::new();
        let mut events = vec![placed_event()];
        store.append(&mut events).unwrap();
        assert_ne!(events[0].timestamp, DateTime::<Utc>::default());
    }
}
