mod event;
mod file_store;
mod mem_store;
#[cfg(feature = "postgres")]
mod pg_store;
mod snapshot;
mod store;

pub use event::{Event, EventData};
pub use file_store::FileStore;
pub use mem_store::MemStore;
#[cfg(feature = "postgres")]
pub use pg_store::PgStore;
pub use snapshot::{MemSnapshotStore, SnapshotStore};
pub use store::{assign_seq_and_timestamp, index_after, Store};

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("limit must be > 0")]
    LimitMustBePositive,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    #[error("corrupt event at offset {offset}: {source}")]
    CorruptEvent { offset: u64, source: bincode::Error },
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
