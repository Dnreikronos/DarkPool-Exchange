//! PgStore integration tests.
//!
//! All tests are `#[ignore]` by default — they require a running Postgres
//! instance reachable via the `DARKPOOL_TEST_PG_URL` env var, e.g.:
//!
//!     docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=dp postgres:16
//!     DARKPOOL_TEST_PG_URL=postgres://postgres:dp@localhost/postgres \
//!       cargo test -p dp-event --features postgres -- --ignored
//!
//! Each test creates a unique schema, runs migrations + the case against it,
//! then drops the schema. Tests are safe to run in parallel on a single DB.

#![cfg(feature = "postgres")]

use std::sync::Arc;

use chrono::DateTime;
use dp_event::{Event, EventData, EventError, PgStore, Store};
use dp_types::EventType;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::ConnectOptions;
use std::str::FromStr;
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

fn pg_url() -> Option<String> {
    std::env::var("DARKPOOL_TEST_PG_URL").ok()
}

/// Per-test sandbox: owns a unique schema and drops it on `Drop`.
struct PgSandbox {
    base_url: String,
    schema: String,
}

impl PgSandbox {
    async fn create() -> Self {
        let base_url = pg_url().expect("DARKPOOL_TEST_PG_URL must be set");
        let schema = format!("dp_test_{}", Uuid::new_v4().simple());

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&base_url)
            .await
            .expect("connect admin");
        sqlx::query(&format!("CREATE SCHEMA \"{}\"", schema))
            .execute(&admin)
            .await
            .expect("create schema");
        admin.close().await;

        Self { base_url, schema }
    }

    async fn store(&self) -> PgStore {
        let schema = self.schema.clone();
        let opts = PgConnectOptions::from_str(&self.base_url)
            .expect("parse url")
            .disable_statement_logging();
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |conn, _meta| {
                let schema = schema.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO \"{}\"", schema))
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(opts)
            .await
            .expect("connect pool");

        PgStore::from_pool(pool).await.expect("build PgStore")
    }
}

impl Drop for PgSandbox {
    fn drop(&mut self) {
        // Best-effort schema drop. Spawn a short-lived runtime since Drop is sync.
        let url = self.base_url.clone();
        let schema = self.schema.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("rt");
            rt.block_on(async {
                if let Ok(pool) = PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&url)
                    .await
                {
                    let _ = sqlx::query(&format!("DROP SCHEMA \"{}\" CASCADE", schema))
                        .execute(&pool)
                        .await;
                    pool.close().await;
                }
            });
        })
        .join();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn append_and_read() {
    let sb = PgSandbox::create().await;
    let store = sb.store().await;

    let mut events = vec![placed_event(), placed_event()];
    store.append(&mut events).unwrap();

    assert_eq!(events[0].seq, 1);
    assert_eq!(events[1].seq, 2);
    assert_eq!(store.last_seq(), 2);

    let read = store.read_from(0, 10).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].seq, 1);
    assert_eq!(read[1].seq, 2);
    assert!(matches!(read[0].data, EventData::OrderPlaced { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn persist_across_reopen() {
    let sb = PgSandbox::create().await;
    {
        let store = sb.store().await;
        let mut events = vec![placed_event(), placed_event()];
        store.append(&mut events).unwrap();
        assert_eq!(store.last_seq(), 2);
    }

    let store = sb.store().await;
    assert_eq!(store.last_seq(), 2);
    let read = store.read_from(0, 10).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].seq, 1);
    assert_eq!(read[1].seq, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn offset_read() {
    let sb = PgSandbox::create().await;
    let store = sb.store().await;
    let mut events = vec![placed_event(), placed_event(), placed_event()];
    store.append(&mut events).unwrap();

    let read = store.read_from(1, 10).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].seq, 2);
    assert_eq!(read[1].seq, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn invalid_limit() {
    let sb = PgSandbox::create().await;
    let store = sb.store().await;
    let err = store.read_from(0, 0).unwrap_err();
    assert!(matches!(err, EventError::LimitMustBePositive));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn concurrent_appends() {
    let sb = PgSandbox::create().await;
    let store = Arc::new(sb.store().await);

    let writers = 8usize;
    let per_writer = 10usize;

    let mut handles = Vec::new();
    for _ in 0..writers {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..per_writer {
                let mut e = vec![placed_event()];
                s.append(&mut e).unwrap();
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let total = (writers * per_writer) as u64;
    assert_eq!(store.last_seq(), total);

    let mut all = store
        .read_from(0, (total as usize) + 1)
        .expect("read all");
    assert_eq!(all.len() as u64, total);

    all.sort_by_key(|e| e.seq);
    for (i, e) in all.iter().enumerate() {
        assert_eq!(e.seq, (i as u64) + 1, "contiguous seq, no gaps/dupes");
    }
}
