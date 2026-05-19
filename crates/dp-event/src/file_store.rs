use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use parking_lot::RwLock;

use crate::{assign_seq_and_timestamp, index_after, Event, EventError, Store};

const MAX_RECORD_BYTES: u32 = 16 * 1024 * 1024;

struct FileInner {
    file: File,
    writer: BufWriter<File>,
    events: Vec<Event>,
    seq: u64,
}

pub struct FileStore {
    inner: RwLock<FileInner>,
}

impl FileStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EventError> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.as_ref())?;

        let (events, seq, good_end) = load_and_truncate(&mut file)?;

        file.set_len(good_end)?;
        file.seek(SeekFrom::Start(good_end))?;

        let writer = BufWriter::new(file.try_clone()?);

        Ok(Self {
            inner: RwLock::new(FileInner {
                file,
                writer,
                events,
                seq,
            }),
        })
    }

    pub fn close(self) -> Result<(), EventError> {
        let mut inner = self.inner.into_inner();
        inner.writer.flush()?;
        inner.file.sync_all()?;
        Ok(())
    }
}

fn load_and_truncate(file: &mut File) -> Result<(Vec<Event>, u64, u64), EventError> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut seq: u64 = 0;
    let mut good_end: u64 = 0;

    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let length = u32::from_be_bytes(len_buf);

        if length == 0 || length > MAX_RECORD_BYTES {
            break;
        }

        let mut payload = vec![0u8; length as usize];
        match reader.read_exact(&mut payload) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }

        let evt: Event =
            bincode::deserialize(&payload).map_err(|source| EventError::CorruptEvent {
                offset: good_end,
                source,
            })?;

        if evt.seq > seq {
            seq = evt.seq;
        }
        events.push(evt);
        good_end += 4 + length as u64;
    }

    Ok((events, seq, good_end))
}

impl Store for FileStore {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError> {
        let mut inner = self.inner.write();

        for e in events.iter_mut() {
            assign_seq_and_timestamp(e, &mut inner.seq);
        }

        for e in events.iter() {
            let payload = bincode::serialize(e)?;
            let length = payload.len() as u32;
            inner.writer.write_all(&length.to_be_bytes())?;
            inner.writer.write_all(&payload)?;
        }

        inner.writer.flush()?;
        inner.file.sync_all()?;

        for e in events.iter() {
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

    fn size_bytes(&self) -> Result<u64, EventError> {
        let inner = self.inner.read();
        Ok(inner.file.metadata()?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventData;
    use chrono::DateTime;
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
                salt_nonce: vec![0u8; 32],
            },
        }
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");

        let store = FileStore::open(&path).unwrap();
        let mut events = vec![placed_event(), placed_event()];
        store.append(&mut events).unwrap();

        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
        assert_eq!(store.last_seq(), 2);

        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 1);
        store.close().unwrap();
    }

    #[test]
    fn persist_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");

        {
            let store = FileStore::open(&path).unwrap();
            let mut events = vec![placed_event(), placed_event()];
            store.append(&mut events).unwrap();
            store.close().unwrap();
        }

        let store = FileStore::open(&path).unwrap();
        assert_eq!(store.last_seq(), 2);
        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 1);
        assert_eq!(read[1].seq, 2);
        store.close().unwrap();
    }

    #[test]
    fn truncate_partial_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");

        {
            let store = FileStore::open(&path).unwrap();
            let mut events = vec![placed_event()];
            store.append(&mut events).unwrap();
            store.close().unwrap();
        }

        // Append garbage partial record
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(&100u32.to_be_bytes()).unwrap();
            f.write_all(&[0xDE, 0xAD]).unwrap();
        }

        let store = FileStore::open(&path).unwrap();
        assert_eq!(store.last_seq(), 1);
        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 1);
        store.close().unwrap();
    }

    #[test]
    fn truncate_oversize_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");

        {
            let store = FileStore::open(&path).unwrap();
            let mut events = vec![placed_event()];
            store.append(&mut events).unwrap();
            store.close().unwrap();
        }

        // Append oversize length header
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            let oversize = MAX_RECORD_BYTES + 1;
            f.write_all(&oversize.to_be_bytes()).unwrap();
        }

        let store = FileStore::open(&path).unwrap();
        assert_eq!(store.last_seq(), 1);
        store.close().unwrap();
    }

    #[test]
    fn invalid_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");
        let store = FileStore::open(&path).unwrap();
        let err = store.read_from(0, 0).unwrap_err();
        assert!(matches!(err, EventError::LimitMustBePositive));
    }
}
