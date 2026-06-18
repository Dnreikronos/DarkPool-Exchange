//! Targeted tests for branches not reached by the happy-path suites:
//! tick-time expiry, recovery edge cases (failing decrypter, commitment
//! mismatch, idempotency, default-ttl, pair-delisted replay, failing
//! aggregator during orphan reaggregation).
//!
//! A tracing subscriber is installed so that `tracing::error!` / `warn!`
//! fields used in error branches actually evaluate at test time. Without
//! it, llvm-cov flags those macro-internal regions as uncovered even
//! though the surrounding branch executes.

use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

static INIT_TRACING: Once = Once::new();
fn init_tracing() {
    INIT_TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("trace"))
            .with_test_writer()
            .try_init();
    });
}

use alloy_primitives::Address;
use chrono::Utc;
use dp_crypto::{CryptoError, DecryptedOrder, Decrypter};
use dp_engine::{Engine, EngineError, PairConfig, PairStatus};
use dp_event::{Event, EventData, EventError, MemStore, Store};
use dp_types::{EventType, Side};
use rust_decimal::Decimal;
use uuid::Uuid;

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

struct AlwaysFailingDecrypter;
impl Decrypter for AlwaysFailingDecrypter {
    fn decrypt<'a>(
        &'a self,
        _ciphertext: &'a [u8],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>,
    > {
        Box::pin(async move { Err(CryptoError::DecryptionFailed("forced".into())) })
    }
}

/// Store that delegates to an inner MemStore for happy ops but fails on
/// read_from / append according to mode flags. Used to exercise the
/// defensive error branches in recover.rs that the happy-path tests
/// never hit.
struct ToggleableStore {
    inner: MemStore,
    fail_read: std::sync::atomic::AtomicBool,
    fail_append: std::sync::atomic::AtomicBool,
}

impl ToggleableStore {
    fn new() -> Self {
        Self {
            inner: MemStore::new(),
            fail_read: std::sync::atomic::AtomicBool::new(false),
            fail_append: std::sync::atomic::AtomicBool::new(false),
        }
    }
    fn enable_read_failures(&self) {
        self.fail_read
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Store for ToggleableStore {
    fn append(&self, events: &mut [Event]) -> Result<(), EventError> {
        if self.fail_append.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(EventError::Io(std::io::Error::other(
                "forced append failure",
            )));
        }
        self.inner.append(events)
    }
    fn read_from(&self, after_seq: u64, limit: usize) -> Result<Vec<Event>, EventError> {
        if self.fail_read.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(EventError::Io(std::io::Error::other("forced read failure")));
        }
        self.inner.read_from(after_seq, limit)
    }
    fn last_seq(&self) -> u64 {
        self.inner.last_seq()
    }
}

struct FailingAggregator;
impl dp_aggregator::ProofAggregator for FailingAggregator {
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [dp_auction::Match],
        _witness: &'a dp_zk::witness::BatchWitness,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<u8>, dp_aggregator::AggregatorError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Err(dp_aggregator::AggregatorError::ProcessFailed {
                exit_code: 1,
                stderr: "forced failure".into(),
            })
        })
    }
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap()
}

fn engine_with_store(store: Arc<dyn Store>) -> Engine {
    let engine = Engine::new(store, Duration::from_millis(50));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    engine
}

async fn place(engine: &Engine, pair: &str, side: Side, price: &str, size: &str, key: &str) {
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: pair.into(),
        side,
        price: dec(price),
        size: dec(size),
        commitment_key: key.into(),
        salt: "ab".repeat(32),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct, None)
        .await
        .unwrap();
}

// ---------- tick: expiry branch ----------

#[tokio::test]
async fn tick_collects_and_emits_expired_orders() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();

    // Place an order with a 1-nanosecond TTL so it is already expired
    // when the next tick runs.
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "ETH/USDC".into(),
        side: Side::Buy,
        price: dec("100"),
        size: dec("1"),
        commitment_key: "expiring".into(),
        salt: "ab".repeat(32),
        ttl: 1,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct, None)
        .await
        .unwrap();

    // Sleep long enough for the order to expire.
    tokio::time::sleep(Duration::from_millis(5)).await;
    engine.run_auction_tick().await;

    // The book no longer holds the expired order and an OrderExpired event
    // was persisted.
    assert_eq!(engine.active_order_count(), 0);
    let events = store.read_from(0, 1024).unwrap();
    let expired = events
        .iter()
        .filter(|e| e.event_type == EventType::OrderExpired)
        .count();
    assert_eq!(expired, 1, "exactly one OrderExpired event expected");
}

// ---------- recover: idempotency ----------

#[tokio::test]
async fn recover_is_idempotent_when_called_twice() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    place(&engine, "ETH/USDC", Side::Buy, "100", "1", "k").await;

    let engine2 = engine_with_store(store);
    engine2.recover().await.unwrap();
    let first_count = engine2.active_order_count();
    // Second call must be a no-op — the recovered flag short-circuits.
    engine2.recover().await.unwrap();
    assert_eq!(engine2.active_order_count(), first_count);
}

// ---------- recover: decrypter failure ----------

#[tokio::test]
async fn recover_returns_decrypt_error_when_decrypter_fails() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    place(&engine, "ETH/USDC", Side::Buy, "100", "1", "k").await;

    let engine2 = Engine::new(store, Duration::from_millis(50));
    engine2.set_decrypter(Arc::new(AlwaysFailingDecrypter));
    let err = engine2.recover().await.unwrap_err();
    assert!(
        matches!(err, EngineError::RecoverDecrypt { .. }),
        "got {err:?}"
    );
}

// ---------- recover: commitment mismatch ----------

#[tokio::test]
async fn recover_returns_commitment_mismatch_for_tampered_event() {
    let store = Arc::new(MemStore::new());

    // Build a valid OrderPlaced ciphertext but inject a wrong commitment so
    // replay's recompute-and-compare diverges.
    let order_id = Uuid::new_v4();
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "ETH/USDC".into(),
        side: Side::Buy,
        price: dec("100"),
        size: dec("1"),
        commitment_key: "k".into(),
        salt: "ab".repeat(32),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    let mut events = [Event {
        seq: 0,
        event_type: EventType::OrderPlaced,
        timestamp: Utc::now(),
        data: EventData::OrderPlaced {
            order_id,
            commitment: vec![0xFF; 32],
            proof: vec![],
            ciphertext: ct,
        },
    }];
    store.append(&mut events).unwrap();

    let engine = Engine::new(store as Arc<dyn Store>, Duration::from_millis(50));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    let err = engine.recover().await.unwrap_err();
    assert!(
        matches!(err, EngineError::RecoverCommitmentMismatch { order_id: id } if id == order_id),
        "got {err:?}"
    );
}

// ---------- recover: default ttl when ciphertext encodes ttl=0 ----------

#[tokio::test]
async fn recover_uses_default_ttl_when_decrypted_ttl_is_zero() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "ETH/USDC".into(),
        side: Side::Buy,
        price: dec("100"),
        size: dec("1"),
        commitment_key: "k".into(),
        salt: "ab".repeat(32),
        ttl: 0,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct, None)
        .await
        .unwrap();

    let engine2 = engine_with_store(store);
    engine2.recover().await.unwrap();
    // Order is recovered with default TTL (24h by default), so it remains active.
    assert_eq!(engine2.active_order_count(), 1);
}

// ---------- recover: cancelled / expired replay ----------

#[tokio::test]
async fn recover_replays_cancelled_orders() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    place(&engine, "ETH/USDC", Side::Buy, "100", "1", "k").await;
    let id = {
        let evs = store.read_from(0, 32).unwrap();
        evs.iter()
            .find_map(|e| match e.data {
                EventData::OrderPlaced { order_id, .. } => Some(order_id),
                _ => None,
            })
            .unwrap()
    };
    engine.cancel_order(id, Some("user".into())).unwrap();
    assert_eq!(engine.active_order_count(), 0);

    let engine2 = engine_with_store(store);
    engine2.recover().await.unwrap();
    assert_eq!(engine2.active_order_count(), 0, "cancel replayed");
}

// ---------- recover: pair lifecycle delisted branch ----------

#[tokio::test]
async fn recover_replays_pair_delisted_event() {
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine.delist_pair("ETH/USDC").unwrap();

    let engine2 = engine_with_store(store);
    engine2.recover().await.unwrap();
    assert_eq!(
        engine2.pair_status("ETH/USDC"),
        Some(PairStatus::Delisted),
        "PairDelisted replayed"
    );
}

// ---------- recover: store error paths ----------

#[tokio::test]
async fn recover_propagates_store_read_error() {
    let store = Arc::new(ToggleableStore::new());
    // Seed one event so reset_projection runs, then make subsequent reads
    // fail. recover() must surface the EventError verbatim.
    let mut events = [Event {
        seq: 0,
        event_type: EventType::PairRegistered,
        timestamp: Utc::now(),
        data: EventData::PairRegistered {
            pair: "ETH/USDC".into(),
            base_token: "0x0".into(),
            quote_token: "0x0".into(),
            min_order_size: dec("0"),
            tick_size: dec("0"),
            auction_interval_ms: None,
            base_decimals: 18,
            quote_decimals: 6,
        },
    }];
    store.append(&mut events).unwrap();
    store.enable_read_failures();

    let engine = Engine::new(store as Arc<dyn Store>, Duration::from_millis(50));
    let err = engine.recover().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("forced read failure"), "got: {msg}");
}

// ---------- recover: pair lifecycle event with unknown pair ----------

#[tokio::test]
async fn recover_pair_suspended_for_unknown_pair_is_noop() {
    // PairSuspended event for a pair that was never registered: the
    // if-let-None branch of the replay arm should run silently rather
    // than crash.
    let store = Arc::new(MemStore::new());
    let mut events = [Event {
        seq: 0,
        event_type: EventType::PairSuspended,
        timestamp: Utc::now(),
        data: EventData::PairSuspended {
            pair: "GHOST/PAIR".into(),
        },
    }];
    store.append(&mut events).unwrap();
    let engine = engine_with_store(store as Arc<dyn Store>);
    engine.recover().await.unwrap();
    assert!(engine.pair_status("GHOST/PAIR").is_none());
}

#[tokio::test]
async fn recover_pair_delisted_for_unknown_pair_is_noop() {
    let store = Arc::new(MemStore::new());
    let mut events = [Event {
        seq: 0,
        event_type: EventType::PairDelisted,
        timestamp: Utc::now(),
        data: EventData::PairDelisted {
            pair: "GHOST/PAIR".into(),
        },
    }];
    store.append(&mut events).unwrap();
    let engine = engine_with_store(store as Arc<dyn Store>);
    engine.recover().await.unwrap();
    assert!(engine.pair_status("GHOST/PAIR").is_none());
}

// ---------- recover: orphan reaggregation hits aggregator-error branch ----------

#[tokio::test]
async fn recover_orphan_reaggregation_error_is_logged_and_skipped() {
    init_tracing();
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let engine = engine_with_store(store.clone());
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    // Wire a failing aggregator on the original engine so the tick produces
    // OrderMatched without a BatchSubmitted — orphan matches in the log.
    engine.set_aggregator(Arc::new(FailingAggregator));
    place(&engine, "ETH/USDC", Side::Buy, "100", "1", "b").await;
    place(&engine, "ETH/USDC", Side::Sell, "100", "1", "s").await;
    engine.run_auction_tick().await;

    // Now recover with a FailingAggregator so the orphan path errors out.
    let engine2 = engine_with_store(store);
    engine2.set_aggregator(Arc::new(FailingAggregator));
    engine2.recover().await.unwrap();
    // No BatchSubmitted should land — the orphan reaggregation failed
    // and the recover loop skipped it without erroring overall.
    assert_eq!(engine2.pending_batch_count(), 0);
}
