use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::runtime::Handle;

use crate::{Event, EventError, Store};

/// Name of the table that holds the event log. Lifted to a constant so the
/// `pg_total_relation_size($1::regclass)` bind on the size-bytes gauge has a
/// single source of truth that a schema rename has to update.
const EVENTS_TABLE: &str = "events";
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// PostgreSQL-backed event store.
///
/// Must be constructed and used inside a multi-threaded tokio runtime —
/// the synchronous `Store` trait methods bridge to async by calling
/// [`tokio::task::block_in_place`] on the captured runtime handle.
/// Each bridged operation is wrapped in a timeout so a stalled database cannot
/// pin callers that hold higher-level engine locks indefinitely.
pub struct PgStore {
    pool: PgPool,
    last_seq: RwLock<u64>,
    handle: Handle,
    operation_timeout: Duration,
}

impl PgStore {
    pub async fn connect(url: &str) -> Result<Self, EventError> {
        let pool = PgPoolOptions::new().max_connections(8).connect(url).await?;
        Self::from_pool(pool).await
    }

    /// Connect using an explicit per-operation timeout for synchronous
    /// `Store` calls such as `append`, `read_from`, and `ping`.
    pub async fn connect_with_timeout(
        url: &str,
        operation_timeout: Duration,
    ) -> Result<Self, EventError> {
        let pool = PgPoolOptions::new().max_connections(8).connect(url).await?;
        Self::from_pool_with_timeout(pool, operation_timeout).await
    }

    pub async fn from_pool(pool: PgPool) -> Result<Self, EventError> {
        Self::from_pool_with_timeout(pool, DEFAULT_OPERATION_TIMEOUT).await
    }

    /// Build from an existing pool using an explicit per-operation timeout.
    pub async fn from_pool_with_timeout(
        pool: PgPool,
        operation_timeout: Duration,
    ) -> Result<Self, EventError> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        let last: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events")
            .fetch_one(&pool)
            .await?;
        Ok(Self {
            pool,
            last_seq: RwLock::new(last as u64),
            handle: Handle::current(),
            operation_timeout: if operation_timeout.is_zero() {
                DEFAULT_OPERATION_TIMEOUT
            } else {
                operation_timeout
            },
        })
    }

    fn block_on_with_timeout<F, T>(&self, operation: &'static str, fut: F) -> Result<T, EventError>
    where
        F: Future<Output = Result<T, EventError>>,
    {
        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                tokio::time::timeout(self.operation_timeout, fut)
                    .await
                    .map_err(|_| EventError::Timeout {
                        operation,
                        timeout: self.operation_timeout,
                    })?
            })
        })
    }
}

impl Store for PgStore {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError> {
        if events.is_empty() {
            return Ok(());
        }

        let pool = self.pool.clone();
        let payloads: Vec<(i16, DateTime<Utc>, Vec<u8>)> = events
            .iter()
            .map(|e| {
                let ts = if e.timestamp == DateTime::<Utc>::default() {
                    Utc::now()
                } else {
                    e.timestamp
                };
                let mut to_persist = e.clone();
                to_persist.timestamp = ts;
                let bytes = bincode::serialize(&to_persist)?;
                Ok::<_, EventError>((e.event_type as i16, ts, bytes))
            })
            .collect::<Result<_, _>>()?;

        let assigned: Vec<(i64, DateTime<Utc>)> = self.block_on_with_timeout("append", async {
            let mut tx = pool.begin().await?;
            let mut out = Vec::with_capacity(payloads.len());
            for (event_type, ts, bytes) in &payloads {
                let row: (i64,) = sqlx::query_as(
                    "INSERT INTO events (event_type, timestamp, data) \
                     VALUES ($1, $2, $3) RETURNING seq",
                )
                .bind(*event_type)
                .bind(*ts)
                .bind(bytes)
                .fetch_one(&mut *tx)
                .await?;
                out.push((row.0, *ts));
            }
            tx.commit().await?;
            Ok(out)
        })?;

        for (e, (seq, ts)) in events.iter_mut().zip(assigned.iter()) {
            e.seq = *seq as u64;
            e.timestamp = *ts;
        }
        if let Some(last) = events.last() {
            *self.last_seq.write() = last.seq;
        }
        Ok(())
    }

    fn read_from(&self, after_seq: u64, limit: usize) -> Result<Vec<Event>, EventError> {
        if limit == 0 {
            return Err(EventError::LimitMustBePositive);
        }

        let pool = self.pool.clone();
        let after = after_seq as i64;
        let lim = limit as i64;

        let rows: Vec<(i64, DateTime<Utc>, Vec<u8>)> =
            self.block_on_with_timeout("read", async {
                sqlx::query_as(
                    "SELECT seq, timestamp, data FROM events \
                     WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
                )
                .bind(after)
                .bind(lim)
                .fetch_all(&pool)
                .await
                .map_err(EventError::from)
            })?;

        let mut out = Vec::with_capacity(rows.len());
        for (seq, ts, bytes) in rows {
            let mut evt: Event = bincode::deserialize(&bytes)?;
            evt.seq = seq as u64;
            evt.timestamp = ts;
            out.push(evt);
        }
        Ok(out)
    }

    fn last_seq(&self) -> u64 {
        *self.last_seq.read()
    }

    fn ping(&self) -> Result<(), EventError> {
        let pool = self.pool.clone();
        self.block_on_with_timeout("ping", async {
            sqlx::query("SELECT 1").execute(&pool).await?;
            Ok(())
        })
    }

    fn compact_before(&self, before_seq: u64) -> Result<(), EventError> {
        if before_seq == 0 {
            return Ok(());
        }
        let pool = self.pool.clone();
        let cutoff = before_seq as i64;
        self.block_on_with_timeout("compact", async {
            sqlx::query("DELETE FROM events WHERE seq < $1")
                .bind(cutoff)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn size_bytes(&self) -> Result<u64, EventError> {
        // Bind the table name through a parameter + ::regclass cast so the
        // value is never spliced into the SQL text. Today `EVENTS_TABLE` is
        // a const, but binding keeps the call site safe against a future
        // refactor that lets the caller pick the table.
        let pool = self.pool.clone();
        let bytes: i64 = self.block_on_with_timeout("size_bytes", async {
            let (n,): (i64,) =
                sqlx::query_as("SELECT pg_total_relation_size($1::regclass)::bigint")
                    .bind(EVENTS_TABLE)
                    .fetch_one(&pool)
                    .await?;
            Ok(n)
        })?;
        Ok(bytes.max(0) as u64)
    }
}
