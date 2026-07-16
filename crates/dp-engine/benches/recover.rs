//! Cold-start `recover()` benchmark — measures replay time with and
//! without a snapshot, then compares.
//!
//! Run a single configuration:
//!
//! ```ignore
//! cargo bench -p dp-engine -- recover
//! ```
//!
//! The default workload is 1 000 synthetic `OrderPlaced` events. The
//! `place_encrypted_order` setup path Poseidon-hashes every order so a
//! 100k-event run is measured in cold minutes wall-clock — issue #25
//! acceptance criteria call out the 100k figure, set `BENCH_EVENTS` to
//! reproduce it:
//!
//! ```ignore
//! BENCH_EVENTS=100000 cargo bench -p dp-engine -- recover
//! ```
//!
//! Bench setup dominates wall time at that scale; the recover() arms
//! themselves remain the meaningful comparison.

use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use dp_crypto::{CryptoError, DecryptedOrder, Decrypter, SnapshotCipher};
use dp_engine::{Engine, SnapshotConfig};
use dp_event::{Event, MemSnapshotStore, MemStore, SnapshotStore, Store};
use dp_types::Side;
use rust_decimal::Decimal;

const DEFAULT_EVENTS: usize = 1_000;

fn bench_events() -> usize {
    std::env::var("BENCH_EVENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_EVENTS)
}

fn canonical_hex32(n: u64) -> String {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&n.to_be_bytes());
    hex::encode(bytes)
}

fn future_expiry_nanos() -> i64 {
    (chrono::Utc::now() + chrono::Duration::hours(12))
        .timestamp_nanos_opt()
        .unwrap()
}

struct JsonDecrypter;

impl Decrypter for JsonDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>,
    > {
        Box::pin(async move { serde_json::from_slice(ciphertext).map_err(CryptoError::from) })
    }
}

fn build_engine(store: Arc<dyn Store>) -> Engine {
    let engine = Engine::new(store, Duration::from_secs(60));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    // Fixed key so the snapshot writer and the `from_snapshot` restorer arm
    // share it and the sealed envelope round-trips (#203).
    engine.set_snapshot_cipher(Some(Arc::new(
        SnapshotCipher::from_bytes(&[7u8; 32]).unwrap(),
    )));
    engine.register_pair_without_event(
        "BTC-USD".into(),
        dp_engine::PairConfig::new(Address::repeat_byte(1), Address::repeat_byte(2)),
    );
    engine
}

async fn populate_store(engine: &Engine, n: usize) {
    for i in 0..n {
        let order = DecryptedOrder {
            trader: Address::ZERO,
            pair: "BTC-USD".into(),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            price: Decimal::new(100 + (i as i64 % 7), 0),
            size: Decimal::new(1, 0),
            commitment_key: format!("ck-{i}"),
            salt: canonical_hex32(i as u64 + 1),
            nonce: canonical_hex32(i as u64 + 1),
            expires_at_unix_nanos: future_expiry_nanos(),
            ttl: 60_000_000_000,
        };
        let ct = serde_json::to_vec(&order).unwrap();
        // Engine recomputes the commitment internally; supplying an
        // empty placeholder here is allowed by `place_encrypted_order`.
        engine
            .place_encrypted_order(vec![0u8; 32], vec![], ct, Address::ZERO)
            .await
            .expect("place");
    }
}

/// Clone every event out of a MemStore. Used so a single populated
/// store can be re-seeded into a fresh MemStore per iteration without
/// re-running the slower `populate_store` path each time.
fn dump_events(store: &dyn Store) -> Vec<Event> {
    let mut out = Vec::new();
    let mut after = 0u64;
    loop {
        let events = store.read_from(after, 4096).expect("read");
        if events.is_empty() {
            break;
        }
        after = events.last().unwrap().seq;
        out.extend(events);
    }
    out
}

fn seed_store_from(events: &[Event]) -> Arc<MemStore> {
    let store = Arc::new(MemStore::new());
    let mut owned: Vec<Event> = events.to_vec();
    store.append(&mut owned).expect("seed");
    store
}

fn bench_recover(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // One-time setup: build a canonical event stream + a snapshot
    // envelope that recovery can re-use across every iteration.
    let events_n = bench_events();
    let (canonical_events, snapshot_envelope, snapshot_seq) = rt.block_on(async {
        let seed_store: Arc<dyn Store> = Arc::new(MemStore::new());
        let engine = build_engine(seed_store.clone());
        populate_store(&engine, events_n).await;
        let events = dump_events(seed_store.as_ref());

        // Capture a snapshot from this populated engine, covering the
        // whole event set. The captured envelope is what each
        // `from_snapshot` iteration will plant into a fresh MemSnapshot.
        let snap_store = Arc::new(MemSnapshotStore::new());
        engine.set_snapshot_store(Some(snap_store.clone()));
        let snapshot_seq = engine.store_last_seq();
        dp_engine::take_snapshot_for_bench(
            &engine,
            snap_store.as_ref(),
            &SnapshotConfig::default(),
            snapshot_seq,
        )
        .expect("snapshot");
        let (seq, envelope) = snap_store
            .read_latest()
            .expect("read snap")
            .expect("snapshot present");
        assert_eq!(seq, snapshot_seq);
        (events, envelope, snapshot_seq)
    });

    let mut group = c.benchmark_group("recover");
    group.sample_size(10);

    group.bench_function("full_replay", |b| {
        b.iter_batched(
            || seed_store_from(&canonical_events),
            |store| {
                rt.block_on(async move {
                    let engine = build_engine(store);
                    engine.recover().await.unwrap();
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("from_snapshot", |b| {
        b.iter_batched(
            || {
                let store = seed_store_from(&canonical_events);
                let snap_store = Arc::new(MemSnapshotStore::new());
                <MemSnapshotStore as SnapshotStore>::write(
                    snap_store.as_ref(),
                    snapshot_seq,
                    &snapshot_envelope,
                )
                .unwrap();
                (store, snap_store)
            },
            |(store, snap_store)| {
                rt.block_on(async move {
                    let engine = build_engine(store);
                    engine.set_snapshot_store(Some(snap_store));
                    engine.recover().await.unwrap();
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_recover);
criterion_main!(benches);
