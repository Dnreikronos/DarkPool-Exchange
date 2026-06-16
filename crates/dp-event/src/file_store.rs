use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use parking_lot::RwLock;

use crate::{assign_seq_and_timestamp, index_after, Event, EventError, Store};

const MAX_RECORD_BYTES: u32 = 16 * 1024 * 1024;

struct FileInner {
    file: File,
    writer: BufWriter<File>,
    events: Vec<Event>,
    seq: u64,
    /// Captured at open time so [`Store::compact_before`] can rewrite
    /// the log atomically without needing the caller to re-supply the
    /// path. Holding the path on the struct keeps the file handle, the
    /// in-memory cache, and the on-disk file in lockstep across the
    /// rewrite.
    path: PathBuf,
}

pub struct FileStore {
    inner: RwLock<FileInner>,
}

impl FileStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EventError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

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
                path,
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

    fn compact_before(&self, before_seq: u64) -> Result<(), EventError> {
        // Holds the inner write lock for the duration of the rewrite +
        // fs::rename. At MVP scale (a few thousand retained events,
        // ~MB of bincode) this is sub-second and the auction tick's
        // append simply blocks behind it; if event-log growth ever
        // pushes compaction into multi-second territory, chunk the
        // rewrite (e.g. drop the lock between batches and re-acquire
        // for the final atomic publish) instead of widening this
        // window further.
        if before_seq == 0 {
            return Ok(());
        }
        let mut inner = self.inner.write();

        // Flush any buffered appends to the live file before we rewrite —
        // otherwise the post-rename handle would be missing the tail of
        // events the caller had appended just before compacting.
        inner.writer.flush()?;
        inner.file.sync_all()?;

        let tmp_path = inner.path.with_extension("compact.tmp");
        {
            let tmp = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            let mut tmp_writer = BufWriter::new(tmp);
            for evt in inner.events.iter().filter(|e| e.seq >= before_seq) {
                let payload = bincode::serialize(evt)?;
                let length = payload.len() as u32;
                tmp_writer.write_all(&length.to_be_bytes())?;
                tmp_writer.write_all(&payload)?;
            }
            tmp_writer.flush()?;
            let tmp_file = tmp_writer
                .into_inner()
                .map_err(|e| EventError::Io(e.into_error()))?;
            tmp_file.sync_all()?;
        }

        // Atomic publish. On Linux `rename` is atomic when source and
        // destination live on the same filesystem — temp file is in the
        // same directory as the log to guarantee that.
        std::fs::rename(&tmp_path, &inner.path)?;

        // Re-open the live handle on the rewritten file; the old handle
        // still points at the now-orphaned inode. seek to EOF and rebuild
        // the writer so subsequent appends land in the right place.
        let mut new_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&inner.path)?;
        let end = new_file.seek(SeekFrom::End(0))?;
        new_file.seek(SeekFrom::Start(end))?;
        let new_writer = BufWriter::new(new_file.try_clone()?);

        inner.file = new_file;
        inner.writer = new_writer;
        inner.events.retain(|e| e.seq >= before_seq);
        Ok(())
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

    #[test]
    fn size_bytes_grows_after_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");
        let store = FileStore::open(&path).unwrap();

        let empty = store.size_bytes().unwrap();
        let mut events = vec![placed_event(), placed_event()];
        store.append(&mut events).unwrap();
        let after = store.size_bytes().unwrap();

        assert!(after > empty, "size {after} should exceed empty {empty}");
    }

    #[test]
    fn compact_rewrites_file_and_preserves_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");
        let store = FileStore::open(&path).unwrap();
        let mut events = vec![placed_event(), placed_event(), placed_event()];
        store.append(&mut events).unwrap();
        let before = store.size_bytes().unwrap();

        store.compact_before(3).unwrap();

        let after = store.size_bytes().unwrap();
        assert!(
            after < before,
            "compacted file {after} not smaller than original {before}"
        );
        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].seq, 3);
        store.close().unwrap();
    }

    #[test]
    fn compact_then_append_then_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.bin");
        {
            let store = FileStore::open(&path).unwrap();
            let mut events = vec![placed_event(), placed_event(), placed_event()];
            store.append(&mut events).unwrap();
            store.compact_before(3).unwrap();
            let mut more = vec![placed_event()];
            store.append(&mut more).unwrap();
            assert_eq!(more[0].seq, 4);
            store.close().unwrap();
        }
        let store = FileStore::open(&path).unwrap();
        let read = store.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 3);
        assert_eq!(read[1].seq, 4);
        store.close().unwrap();
    }
}
