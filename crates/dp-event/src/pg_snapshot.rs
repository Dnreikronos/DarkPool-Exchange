use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::runtime::Handle;

use crate::{EventError, SnapshotStore};

/// PostgreSQL-backed [`SnapshotStore`]. Same runtime requirement as
/// [`crate::PgStore`] — methods are synchronous and bridge to async via
/// `tokio::task::block_in_place` on the captured runtime handle, so
/// every caller must be inside a multi-threaded tokio runtime.
pub struct PgSnapshotStore {
    pool: PgPool,
    handle: Handle,
}

impl PgSnapshotStore {
    pub async fn connect(url: &str) -> Result<Self, EventError> {
        let pool = PgPoolOptions::new().max_connections(4).connect(url).await?;
        Self::from_pool(pool).await
    }

    pub async fn from_pool(pool: PgPool) -> Result<Self, EventError> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self {
            pool,
            handle: Handle::current(),
        })
    }
}

impl SnapshotStore for PgSnapshotStore {
    fn write(&self, seq: u64, envelope: &[u8]) -> Result<(), EventError> {
        let pool = self.pool.clone();
        let seq = seq as i64;
        let bytes = envelope.to_vec();
        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query(
                    "INSERT INTO snapshots (seq, envelope) VALUES ($1, $2) \
                     ON CONFLICT (seq) DO UPDATE SET envelope = EXCLUDED.envelope",
                )
                .bind(seq)
                .bind(bytes)
                .execute(&pool)
                .await?;
                Ok::<_, EventError>(())
            })
        })
    }

    fn read_latest(&self) -> Result<Option<(u64, Vec<u8>)>, EventError> {
        let pool = self.pool.clone();
        let row: Option<(i64, Vec<u8>)> = tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query_as("SELECT seq, envelope FROM snapshots ORDER BY seq DESC LIMIT 1")
                    .fetch_optional(&pool)
                    .await
            })
        })?;
        Ok(row.map(|(s, e)| (s as u64, e)))
    }

    fn read_at(&self, seq: u64) -> Result<Option<Vec<u8>>, EventError> {
        let pool = self.pool.clone();
        let key = seq as i64;
        let row: Option<(Vec<u8>,)> = tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query_as("SELECT envelope FROM snapshots WHERE seq = $1")
                    .bind(key)
                    .fetch_optional(&pool)
                    .await
            })
        })?;
        Ok(row.map(|(e,)| e))
    }

    fn list_seqs(&self) -> Result<Vec<u64>, EventError> {
        let pool = self.pool.clone();
        let rows: Vec<(i64,)> = tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query_as("SELECT seq FROM snapshots ORDER BY seq ASC")
                    .fetch_all(&pool)
                    .await
            })
        })?;
        Ok(rows.into_iter().map(|(s,)| s as u64).collect())
    }

    fn delete_before(&self, before_seq: u64) -> Result<(), EventError> {
        let pool = self.pool.clone();
        let cutoff = before_seq as i64;
        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query("DELETE FROM snapshots WHERE seq < $1")
                    .bind(cutoff)
                    .execute(&pool)
                    .await?;
                Ok::<_, EventError>(())
            })
        })
    }
}
