use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{EventError, SnapshotStore};

const SNAP_EXT: &str = "snap";
const SEQ_DIGITS: usize = 20;

/// On-disk [`SnapshotStore`] that keeps one file per snapshot under a
/// directory. Filenames are `{seq:020}.snap`, so a lexicographic sort
/// matches numeric seq order and the latest snapshot is the last entry.
///
/// Writes go through a `.tmp` sibling and `fs::rename` so a crash
/// mid-write does not leave a torn snapshot visible to the next reader.
pub struct FileSnapshotStore {
    dir: PathBuf,
}

impl FileSnapshotStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, EventError> {
        let dir = dir.as_ref().to_path_buf();
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(Self { dir })
    }

    fn path_for(&self, seq: u64) -> PathBuf {
        self.dir
            .join(format!("{seq:0width$}.{SNAP_EXT}", width = SEQ_DIGITS))
    }
}

fn parse_seq_from_filename(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(&format!(".{SNAP_EXT}"))?;
    if stem.len() != SEQ_DIGITS {
        return None;
    }
    stem.parse::<u64>().ok()
}

impl SnapshotStore for FileSnapshotStore {
    fn write(&self, seq: u64, envelope: &[u8]) -> Result<(), EventError> {
        let target = self.path_for(seq);
        let tmp = target.with_extension("tmp");
        {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            let mut writer = std::io::BufWriter::new(file);
            writer.write_all(envelope)?;
            writer.flush()?;
            let file = writer
                .into_inner()
                .map_err(|e| EventError::Io(e.into_error()))?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &target)?;
        // Best-effort fsync of the directory so the rename survives a
        // power loss before the parent dirent has been flushed. Errors
        // here are not fatal — the snapshot itself is durable.
        if let Ok(dir) = File::open(&self.dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    fn read_latest(&self) -> Result<Option<(u64, Vec<u8>)>, EventError> {
        let seqs = self.list_seqs()?;
        let Some(&seq) = seqs.last() else {
            return Ok(None);
        };
        let bytes = fs::read(self.path_for(seq))?;
        Ok(Some((seq, bytes)))
    }

    fn read_at(&self, seq: u64) -> Result<Option<Vec<u8>>, EventError> {
        match fs::read(self.path_for(seq)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list_seqs(&self) -> Result<Vec<u64>, EventError> {
        let mut out = Vec::new();
        let read_dir = match fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in read_dir {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(seq) = parse_seq_from_filename(name) {
                out.push(seq);
            }
        }
        out.sort();
        Ok(out)
    }

    fn delete_before(&self, before_seq: u64) -> Result<(), EventError> {
        for seq in self.list_seqs()? {
            if seq < before_seq {
                let path = self.path_for(seq);
                if let Err(e) = fs::remove_file(&path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(42, b"payload").unwrap();
        let (seq, bytes) = store.read_latest().unwrap().unwrap();
        assert_eq!(seq, 42);
        assert_eq!(bytes, b"payload");
    }

    #[test]
    fn read_latest_picks_highest_seq() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(1, b"a").unwrap();
        store.write(100, b"big").unwrap();
        store.write(50, b"mid").unwrap();
        let (seq, bytes) = store.read_latest().unwrap().unwrap();
        assert_eq!(seq, 100);
        assert_eq!(bytes, b"big");
    }

    #[test]
    fn list_seqs_returns_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(30, b"x").unwrap();
        store.write(10, b"x").unwrap();
        store.write(20, b"x").unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![10, 20, 30]);
    }

    #[test]
    fn delete_before_drops_old_snaps_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        for s in [1u64, 5, 10] {
            store.write(s, b"x").unwrap();
        }
        store.delete_before(10).unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![10]);
    }

    #[test]
    fn read_latest_empty_dir_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        assert!(store.read_latest().unwrap().is_none());
    }

    #[test]
    fn ignores_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(7, b"x").unwrap();
        std::fs::write(dir.path().join("README.txt"), "hi").unwrap();
        std::fs::write(dir.path().join("00000000000000000099.tmp"), "stale").unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![7]);
    }

    #[test]
    fn read_at_missing_seq_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        assert!(store.read_at(999).unwrap().is_none());
    }

    #[test]
    fn read_at_existing_seq_returns_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(42, b"hello").unwrap();
        let bytes = store.read_at(42).unwrap().expect("should be Some");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn delete_before_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::open(dir.path()).unwrap();
        store.write(5, b"x").unwrap();
        store.delete_before(3).unwrap();
        store.delete_before(3).unwrap();
        assert_eq!(store.list_seqs().unwrap(), vec![5]);
    }

    #[test]
    fn list_seqs_on_nonexistent_subdir_returns_empty() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("does_not_exist");
        let store = FileSnapshotStore { dir: missing };
        assert!(store.list_seqs().unwrap().is_empty());
    }
}
