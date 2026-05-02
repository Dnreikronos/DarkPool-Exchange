use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::runtime::Handle;

use crate::{Event, EventError, Store};

/// PostgreSQL-backed event store.
///
/// Must be constructed and used inside a multi-threaded tokio runtime —
/// the synchronous `Store` trait methods bridge to async by calling
/// [`tokio::task::block_in_place`] on the captured runtime handle.
pub struct PgStore {
    pool: PgPool,
    last_seq: RwLock<u64>,
    handle: Handle,
}

impl PgStore {
    pub async fn connect(url: &str) -> Result<Self, EventError> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await?;
        Self::from_pool(pool).await
    }

    pub async fn from_pool(pool: PgPool) -> Result<Self, EventError> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        let last: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events")
            .fetch_one(&pool)
            .await?;
        Ok(Self {
            pool,
            last_seq: RwLock::new(last as u64),
            handle: Handle::current(),
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

        let assigned: Vec<(i64, DateTime<Utc>)> = tokio::task::block_in_place(|| {
            self.handle.block_on(async {
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
                Ok::<_, EventError>(out)
            })
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

        let rows: Vec<(i64, DateTime<Utc>, Vec<u8>)> = tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                sqlx::query_as(
                    "SELECT seq, timestamp, data FROM events \
                     WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
                )
                .bind(after)
                .bind(lim)
                .fetch_all(&pool)
                .await
            })
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
}
