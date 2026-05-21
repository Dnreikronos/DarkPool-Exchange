use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use dp_aggregator::ProofAggregator;
use dp_crypto::DecryptedOrder;
use dp_event::{EventData, FileStore, MemStore, Store};
use dp_settlement::Submitter;
use dp_types::{EventType, Side};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::test_helpers::{
    count_events, last_proof_for_batch, place_plaintext_order, BlockingAggregator,
    FailingAggregator, StubAggregator, StubSubmitter, XorDecrypter,
};
use crate::Engine;

fn make_engine() -> (Engine, Arc<MemStore>) {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    engine.register_pair_without_event("BTC-USD".into(), crate::state::PairConfig::default());
    (engine, store)
}

fn dec(n: i64) -> Decimal {
    Decimal::new(n, 0)
}

fn register_btc_usd(engine: &Engine) {
    engine.register_pair_without_event("BTC-USD".into(), crate::state::PairConfig::default());
}

// --- engine_test.go ports ---

#[tokio::test]
async fn place_order_succeeds() {
    let (engine, _store) = make_engine();
    let order = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "key1",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    assert_eq!(order.pair, "BTC-USD");
    assert_eq!(order.side, Side::Buy);
    assert_eq!(engine.active_order_count(), 1);
}

#[tokio::test]
async fn place_order_validation() {
    let (engine, _) = make_engine();
    // empty pair
    let r = place_plaintext_order(
        &engine,
        "",
        Side::Buy,
        dec(100),
        dec(1),
        "k",
        Duration::from_secs(60),
    )
    .await;
    assert!(r.is_err());

    // zero price
    let r = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        Decimal::ZERO,
        dec(1),
        "k",
        Duration::from_secs(60),
    )
    .await;
    assert!(r.is_err());

    // zero size
    let r = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        Decimal::ZERO,
        "k",
        Duration::from_secs(60),
    )
    .await;
    assert!(r.is_err());

    // empty commitment key
    let r = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "",
        Duration::from_secs(60),
    )
    .await;
    assert!(r.is_err());
}

#[tokio::test]
async fn cancel_order() {
    let (engine, _) = make_engine();
    let order = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "k",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.cancel_order(order.id, None).unwrap();
    assert_eq!(engine.active_order_count(), 0);
    assert!(engine.get_order(order.id).is_none());
}

#[tokio::test]
async fn cancel_unknown_order_errors() {
    let (engine, _) = make_engine();
    assert!(engine.cancel_order(Uuid::new_v4(), None).is_err());
}

#[tokio::test]
async fn run_auction_tick_produces_notification() {
    let (engine, _) = make_engine();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(5),
        "buyer",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(5),
        "seller",
        Duration::from_secs(60),
    )
    .await
    .unwrap();

    let notifs = engine.run_auction_tick().await;
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0].pair, "BTC-USD");
    assert_eq!(notifs[0].matched_volume, dec(5));
    assert_eq!(notifs[0].match_count, 1);
}

#[tokio::test]
async fn subscribe_receives_notifications() {
    let (engine, _) = make_engine();
    let mut rx = engine.subscribe();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(2),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(2),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;

    let n = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("notification timeout")
        .expect("recv ok");
    assert_eq!(n.pair, "BTC-USD");
    assert_eq!(n.match_count, 1);
}

#[tokio::test]
async fn start_stops_on_cancel() {
    let (engine, _) = make_engine();
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel2 = cancel.clone();
    let engine2 = engine.clone();
    let handle = tokio::spawn(async move { engine2.start(cancel2).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("start did not exit")
        .unwrap();
}

#[tokio::test]
async fn get_auction_history_filters_and_limits() {
    let (engine, _) = make_engine();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b1",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s1",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;

    let hist = engine.get_auction_history(Some("BTC-USD"), 10).unwrap();
    assert_eq!(hist.len(), 1);
    let hist = engine.get_auction_history(Some("ETH-USD"), 10).unwrap();
    assert!(hist.is_empty());
    let hist = engine.get_auction_history(None, 10).unwrap();
    assert_eq!(hist.len(), 1);
}

#[tokio::test]
async fn place_encrypted_order_noop_round_trip() {
    let (engine, _) = make_engine();
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "BTC-USD".into(),
        side: Side::Buy,
        price: dec(100),
        size: dec(1),
        commitment_key: "k".into(),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    let order = engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct)
        .await
        .unwrap();
    assert_eq!(order.pair, "BTC-USD");
}

#[tokio::test]
async fn place_encrypted_order_uses_engine_derived_commitment() {
    let (engine, store) = make_engine();
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "BTC-USD".into(),
        side: Side::Buy,
        price: dec(100),
        size: dec(1),
        commitment_key: "k".into(),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    let order = engine
        .place_encrypted_order(vec![0xAB; 32], vec![], ct)
        .await
        .expect("engine no longer rejects on client commitment");

    let events = store.read_from(0, 16).unwrap();
    let (persisted, nonce) = events
        .iter()
        .find_map(|e| match &e.data {
            EventData::OrderPlaced {
                commitment,
                salt_nonce,
                ..
            } => {
                let n: [u8; 32] = salt_nonce.as_slice().try_into().unwrap();
                Some((commitment.clone(), n))
            }
            _ => None,
        })
        .expect("OrderPlaced");

    let expected = crate::engine::recompute_persisted_commitment(
        order.id,
        &d.commitment_key,
        d.side as u8,
        d.price,
        d.size,
        &nonce,
    );

    assert_eq!(persisted, expected.unwrap().to_vec());
    assert_ne!(
        persisted,
        vec![0xAB; 32],
        "engine must not echo client value"
    );
}

#[tokio::test]
async fn place_encrypted_order_bad_ciphertext() {
    let (engine, _) = make_engine();
    let r = engine
        .place_encrypted_order(vec![0u8; 32], vec![], b"not json".to_vec())
        .await;
    assert!(r.is_err());
}

/// Regression: admin registers via `Pair::parse` (canonical upper-case),
/// but traders may send any casing. The trader path must canonicalise
/// before the registry lookup — otherwise `eth/usdc` 404s against an
/// `ETH/USDC` entry.
#[tokio::test]
async fn place_encrypted_order_canonicalises_lowercase_pair() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_millis(50));
    engine
        .register_pair_with_event("ETH/USDC", crate::state::PairConfig::default())
        .unwrap();

    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: "eth/usdc".into(),
        side: Side::Buy,
        price: dec(100),
        size: dec(1),
        commitment_key: "k".into(),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    let order = engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct)
        .await
        .expect("lowercase pair canonicalises and matches registry");

    assert_eq!(order.pair, "ETH/USDC");
    let (bids, _) = engine.get_order_book("ETH/USDC");
    assert_eq!(bids.len(), 1);
}

#[tokio::test]
async fn event_store_contains_no_plaintext() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    let key = b"darkpool-canary-key";
    let xor = Arc::new(XorDecrypter::new(key));
    engine.set_decrypter(xor.clone());

    let pair = "SUPER-SECRET-PAIR";
    let commitment_key = "SUPER-SECRET-KEY";
    engine.register_pair_without_event(pair.into(), crate::state::PairConfig::default());
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: pair.into(),
        side: Side::Buy,
        price: Decimal::new(123456789, 4),
        size: Decimal::new(987654321, 4),
        commitment_key: commitment_key.into(),
        ttl: 60_000_000_000,
    };
    let plain = serde_json::to_vec(&d).unwrap();
    let ct = xor.encrypt(&plain);
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct)
        .await
        .unwrap();

    // Walk every persisted event, serialize, assert plaintext does not leak.
    let events = store.read_from(0, 1024).unwrap();
    for ev in events {
        let bytes = serde_json::to_vec(&ev).unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(!s.contains(pair), "pair leaked in event: {s}");
        assert!(
            !s.contains(commitment_key),
            "commitment_key leaked in event: {s}"
        );
        assert!(
            !s.contains("123456789"),
            "price digits leaked in event: {s}"
        );
        assert!(!s.contains("987654321"), "size digits leaked in event: {s}");
    }
}

#[tokio::test]
async fn recover_from_mem_store() {
    let store = Arc::new(MemStore::new());
    let engine1 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine1);
    place_plaintext_order(
        &engine1,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "k1",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine1,
        "BTC-USD",
        Side::Sell,
        dec(110),
        dec(1),
        "k2",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    assert_eq!(engine1.active_order_count(), 2);

    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine2);
    engine2.recover().await.unwrap();
    assert_eq!(engine2.active_order_count(), 2);
}

#[tokio::test]
async fn recover_preserves_pair_suspended_status() {
    let store = Arc::new(MemStore::new());
    let engine1 = Engine::new(store.clone(), Duration::from_millis(50));
    engine1
        .register_pair_with_event("BTC-USD", crate::state::PairConfig::default())
        .expect("register");
    engine1.suspend_pair("BTC-USD").expect("suspend");

    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    engine2.recover().await.unwrap();

    let pairs = engine2.list_pairs();
    let status = pairs
        .iter()
        .find(|(p, _)| p == "BTC-USD")
        .map(|(_, c)| c.status)
        .expect("pair must exist after recovery");
    assert!(
        matches!(status, crate::state::PairStatus::Suspended),
        "recovered pair status must be Suspended, got {status:?}",
    );
}

#[tokio::test]
async fn recover_preserves_pair_delisted_status() {
    let store = Arc::new(MemStore::new());
    let engine1 = Engine::new(store.clone(), Duration::from_millis(50));
    engine1
        .register_pair_with_event("BTC-USD", crate::state::PairConfig::default())
        .expect("register");
    engine1.delist_pair("BTC-USD").expect("delist");

    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    engine2.recover().await.unwrap();

    let pairs = engine2.list_pairs();
    let status = pairs
        .iter()
        .find(|(p, _)| p == "BTC-USD")
        .map(|(_, c)| c.status)
        .expect("pair must exist after recovery");
    assert!(
        matches!(status, crate::state::PairStatus::Delisted),
        "recovered pair status must be Delisted, got {status:?}",
    );
}

#[tokio::test]
async fn recover_from_file_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.bin");
    let store: Arc<dyn dp_event::Store> = Arc::new(FileStore::open(&path).unwrap());

    let engine1 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine1);
    place_plaintext_order(
        &engine1,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "k1",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine1,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "k2",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine1.run_auction_tick().await;
    drop(engine1);
    drop(store);

    let store2: Arc<dyn dp_event::Store> = Arc::new(FileStore::open(&path).unwrap());
    let engine2 = Engine::new(store2, Duration::from_millis(50));
    register_btc_usd(&engine2);
    engine2.recover().await.unwrap();
    let hist = engine2.get_auction_history(None, 10).unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].matched_volume, dec(1));
}

#[tokio::test]
async fn slow_aggregator_does_not_block_place_order() {
    let (engine, _) = make_engine();
    let blocking = BlockingAggregator::new();
    engine.set_aggregator(blocking.clone() as Arc<dyn ProofAggregator>);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();

    let engine2 = engine.clone();
    let tick = tokio::spawn(async move { engine2.run_auction_tick().await });

    blocking.wait_started().await;

    // While aggregator is blocked, place_order must succeed.
    let placed = place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(99),
        dec(1),
        "b2",
        Duration::from_secs(60),
    )
    .await;
    assert!(placed.is_ok());

    blocking.release();
    tick.await.unwrap();
}

// --- batch_test.go ports ---

#[tokio::test]
async fn batch_lifecycle_noop_submitter() {
    let (engine, _) = make_engine();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    assert_eq!(engine.pending_batch_count(), 0);
}

/// A recovered `BatchSubmitted`-but-not-`BatchConfirmed` batch is
/// *poisoned*: the witness secrets that produced its public_inputs were
/// wiped on restart, so resubmit must refuse to send those zero pubs
/// on-chain (Groth16 would reject; gas wasted; silent failure-loop).
///
/// FUTURE: when `BatchSubmitted` persists `public_inputs`, recovery can
/// reconstruct the full submit payload and this test should flip to
/// asserting a successful resubmit.
#[tokio::test]
async fn batch_lifecycle_recovered_batch_is_poisoned_until_pubs_persist() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine);
    let stub = Arc::new(StubSubmitter::new());
    stub.set_fail(true);
    engine.set_submitter(stub.clone() as Arc<dyn Submitter>);
    engine.set_retry_backoff(Duration::ZERO, Duration::ZERO);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    assert_eq!(engine.pending_batch_count(), 1);

    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine2);
    let ok = Arc::new(StubSubmitter::new());
    engine2.set_submitter(ok.clone() as Arc<dyn Submitter>);
    engine2.set_retry_backoff(Duration::ZERO, Duration::ZERO);
    engine2.recover().await.unwrap();
    assert_eq!(engine2.pending_batch_count(), 1);

    engine2.resubmit_pending().await;
    // Batch must remain pending: the guard refuses to submit a zero-pubs
    // recovered batch, and the submitter must NEVER be called for it.
    assert_eq!(engine2.pending_batch_count(), 1);
    assert_eq!(
        ok.calls(),
        0,
        "submitter must not be invoked for a poisoned recovered batch"
    );
}

#[tokio::test]
async fn recover_re_aggregates_orphan_matches() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine);
    let failing = Arc::new(FailingAggregator);
    engine.set_aggregator(failing as Arc<dyn ProofAggregator>);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();

    engine.run_auction_tick().await;
    // Aggregation failed -> no BatchSubmitted persisted, but OrderMatched exists.
    let order_matched_count = count_events(store.as_ref(), EventType::OrderMatched);
    assert!(order_matched_count >= 1);
    let batch_submitted_count = count_events(store.as_ref(), EventType::BatchSubmitted);
    assert_eq!(batch_submitted_count, 0);

    // Recover with working aggregator.
    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine2);
    let stub_agg = Arc::new(StubAggregator::new(vec![0xab; 32]));
    engine2.set_aggregator(stub_agg.clone() as Arc<dyn ProofAggregator>);
    engine2.recover().await.unwrap();

    assert!(stub_agg.calls() >= 1);
    assert_eq!(engine2.pending_batch_count(), 1);
    let batch_submitted_count = count_events(store.as_ref(), EventType::BatchSubmitted);
    assert_eq!(batch_submitted_count, 1);
}

#[tokio::test]
async fn batch_lifecycle_recover_from_file_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.bin");
    let store: Arc<dyn dp_event::Store> = Arc::new(FileStore::open(&path).unwrap());

    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine);
    let failing_stub = Arc::new(StubSubmitter::new());
    failing_stub.set_fail(true);
    engine.set_submitter(failing_stub as Arc<dyn Submitter>);
    engine.set_retry_backoff(Duration::ZERO, Duration::ZERO);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    assert_eq!(engine.pending_batch_count(), 1);
    drop(engine);
    drop(store);

    let store2: Arc<dyn dp_event::Store> = Arc::new(FileStore::open(&path).unwrap());
    let engine2 = Engine::new(store2, Duration::from_millis(50));
    register_btc_usd(&engine2);
    let ok = Arc::new(StubSubmitter::new());
    engine2.set_submitter(ok.clone() as Arc<dyn Submitter>);
    engine2.set_retry_backoff(Duration::ZERO, Duration::ZERO);
    engine2.recover().await.unwrap();
    assert_eq!(engine2.pending_batch_count(), 1);
    engine2.resubmit_pending().await;
    // FUTURE: once `BatchSubmitted` persists `public_inputs`, the
    // recovered batch can actually resubmit and `pending_batch_count`
    // should drop to 0. Today the poison guard refuses to submit a
    // zero-pubs batch — see `batch_lifecycle_recovered_batch_is_poisoned_until_pubs_persist`.
    assert_eq!(engine2.pending_batch_count(), 1);
    assert_eq!(ok.calls(), 0);
}

#[tokio::test]
async fn batch_lifecycle_retries_on_next_tick() {
    let (engine, _) = make_engine();
    let stub = Arc::new(StubSubmitter::new());
    stub.set_fail(true);
    engine.set_submitter(stub.clone() as Arc<dyn Submitter>);
    engine.set_retry_backoff(Duration::ZERO, Duration::ZERO);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    let calls_after_first = stub.calls();
    assert!(calls_after_first >= 1);
    assert_eq!(engine.pending_batch_count(), 1);

    engine.run_auction_tick().await;
    assert!(stub.calls() > calls_after_first);
}

#[tokio::test]
async fn batch_lifecycle_proof_persisted_and_reused_on_resubmit() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine);
    let agg_proof = vec![0xCD; 32];
    let stub_agg = Arc::new(StubAggregator::new(agg_proof.clone()));
    engine.set_aggregator(stub_agg.clone() as Arc<dyn ProofAggregator>);
    let stub_sub = Arc::new(StubSubmitter::new());
    stub_sub.set_fail(true);
    engine.set_submitter(stub_sub.clone() as Arc<dyn Submitter>);
    engine.set_retry_backoff(Duration::ZERO, Duration::ZERO);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    assert_eq!(stub_agg.calls(), 1);
    let last_proof = stub_sub.last_proof.lock().clone();
    assert_eq!(last_proof, agg_proof);

    // Recover with NoopAggregator: proof should come from event log, not re-aggregation.
    let engine2 = Engine::new(store.clone(), Duration::from_millis(50));
    register_btc_usd(&engine2);
    let ok = Arc::new(StubSubmitter::new());
    engine2.set_submitter(ok.clone() as Arc<dyn Submitter>);
    engine2.set_retry_backoff(Duration::ZERO, Duration::ZERO);
    engine2.recover().await.unwrap();
    // Verify the persisted proof bytes survived recovery via the
    // in-memory `PendingBatch` — direct state inspection avoids needing
    // an actual submit (the poison guard would block it on zero pubs).
    let recovered_proof = {
        let state = engine2.inner.state.lock();
        state
            .pending_batches
            .values()
            .next()
            .expect("recovered pending batch")
            .proof
            .clone()
    };
    assert_eq!(
        recovered_proof, agg_proof,
        "BatchSubmitted event must preserve the original aggregator proof"
    );
    engine2.resubmit_pending().await;
    // Poison guard fires; submitter is never called.
    assert_eq!(ok.calls(), 0);
}

#[tokio::test]
async fn batch_lifecycle_start_replays_pending() {
    let (engine, _) = make_engine();
    let stub = Arc::new(StubSubmitter::new());
    stub.set_fail(true);
    engine.set_submitter(stub.clone() as Arc<dyn Submitter>);
    engine.set_retry_backoff(Duration::ZERO, Duration::ZERO);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;
    assert_eq!(engine.pending_batch_count(), 1);

    // Flip submitter to ok; start should resubmit on boot.
    let ok = Arc::new(StubSubmitter::new());
    engine.set_submitter(ok.clone() as Arc<dyn Submitter>);

    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel2 = cancel.clone();
    let engine2 = engine.clone();
    let h = tokio::spawn(async move { engine2.start(cancel2).await });
    tokio::time::sleep(Duration::from_millis(80)).await;
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), h).await;

    assert_eq!(engine.pending_batch_count(), 0);
    assert!(ok.calls() >= 1);
}

#[tokio::test]
async fn last_proof_helper_finds_persisted_proof() {
    let (engine, store) = make_engine();
    let stub_agg = Arc::new(StubAggregator::new(vec![0x42; 16]));
    engine.set_aggregator(stub_agg.clone() as Arc<dyn ProofAggregator>);

    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Buy,
        dec(100),
        dec(1),
        "b",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    place_plaintext_order(
        &engine,
        "BTC-USD",
        Side::Sell,
        dec(100),
        dec(1),
        "s",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    engine.run_auction_tick().await;

    // Find any BatchSubmitted's batch_id by scanning store.
    let events = store.read_from(0, 1024).unwrap();
    let batch_id = events
        .iter()
        .find_map(|e| match &e.data {
            EventData::BatchSubmitted { batch_id, .. } => Some(*batch_id),
            _ => None,
        })
        .expect("batch submitted");
    let proof = last_proof_for_batch(store.as_ref(), batch_id).expect("proof found");
    assert_eq!(proof, vec![0x42; 16]);
}

// --- Full pipeline integration test ---
//
// Real ECIES decrypter + StubAggregator + NoopSubmitter, ending with
// engine's BatchSink path (Watcher would call on_batch_settled identically).

#[tokio::test]
async fn full_pipeline_encrypted_order_to_settlement() {
    use dp_settlement::{BatchSink, NoopSubmitter};
    use k256::ecdsa::SigningKey;
    use std::io::Write;

    let sk = SigningKey::random(&mut rand::thread_rng());
    let sk_hex = hex::encode(sk.to_bytes());
    let pk_bytes = sk.verifying_key().to_sec1_bytes();

    let mut keyfile = tempfile::NamedTempFile::new().unwrap();
    keyfile.write_all(sk_hex.as_bytes()).unwrap();
    keyfile.flush().unwrap();

    let decrypter = Arc::new(dp_crypto::EciesDecrypter::from_file(keyfile.path()).unwrap());
    let proof_bytes = vec![0xab; 24];
    let aggregator = Arc::new(StubAggregator::new(proof_bytes.clone()));
    let submitter = Arc::new(NoopSubmitter);

    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    engine.register_pair_without_event("ETH-USD".into(), crate::state::PairConfig::default());
    engine.set_decrypter(decrypter);
    engine.set_aggregator(aggregator.clone());
    engine.set_submitter(submitter);

    let encrypt_order = |side: Side, price: Decimal, key: &str| -> (Vec<u8>, Vec<u8>) {
        let order = DecryptedOrder {
            trader: Address::ZERO,
            pair: "ETH-USD".into(),
            side,
            price,
            size: dec(1),
            commitment_key: key.into(),
            ttl: 60_000_000_000,
        };
        let plaintext = serde_json::to_vec(&order).unwrap();
        let ciphertext = ecies::encrypt(&pk_bytes, &plaintext).unwrap();
        (vec![0u8; 32], ciphertext)
    };

    let (bid_commit, bid_ct) = encrypt_order(Side::Buy, dec(2000), "bid-key");
    let (ask_commit, ask_ct) = encrypt_order(Side::Sell, dec(1900), "ask-key");

    let bid_order = engine
        .place_encrypted_order(bid_commit, vec![], bid_ct)
        .await
        .unwrap();
    let ask_order = engine
        .place_encrypted_order(ask_commit, vec![], ask_ct)
        .await
        .unwrap();
    assert_eq!(bid_order.pair, "ETH-USD");
    assert_eq!(ask_order.side, Side::Sell);
    assert_eq!(engine.active_order_count(), 2);

    let mut rx = engine.subscribe();
    let notes = engine.run_auction_tick().await;
    assert_eq!(notes.len(), 1, "expected 1 auction notification");
    let note = rx.try_recv().expect("broadcast notification");
    assert_eq!(note.pair, "ETH-USD");
    assert_eq!(note.match_count, 1);

    assert_eq!(count_events(store.as_ref(), EventType::AuctionExecuted), 1);
    assert_eq!(count_events(store.as_ref(), EventType::OrderMatched), 1);
    assert_eq!(count_events(store.as_ref(), EventType::BatchSubmitted), 1);
    assert_eq!(aggregator.calls(), 1);

    // Watcher path: pull batch_id from BatchSubmitted, drive BatchSink.
    let events = store.read_from(0, 1024).unwrap();
    let batch_id = events
        .iter()
        .find_map(|e| match &e.data {
            EventData::BatchSubmitted { batch_id, .. } => Some(*batch_id),
            _ => None,
        })
        .expect("batch submitted");
    assert_eq!(
        last_proof_for_batch(store.as_ref(), batch_id).unwrap(),
        proof_bytes
    );
    // NoopSubmitter succeeds inline → BatchConfirmed persisted, pending drained.
    assert_eq!(count_events(store.as_ref(), EventType::BatchConfirmed), 1);
    assert_eq!(engine.pending_batch_count(), 0);

    // Watcher → engine sink: emits BatchSettled regardless of pending state.
    BatchSink::on_batch_settled(&engine, batch_id, 12345, "0xdeadbeef".into())
        .await
        .unwrap();
    assert_eq!(count_events(store.as_ref(), EventType::BatchSettled), 1);

    // Privacy: plaintext keys never appear in serialized event log.
    let serialized = bincode::serialize(&events).unwrap();
    assert!(
        !serialized.windows(7).any(|w| w == b"bid-key"),
        "bid commitment_key leaked in events"
    );
    assert!(
        !serialized.windows(7).any(|w| w == b"ask-key"),
        "ask commitment_key leaked in events"
    );
}

#[tokio::test]
async fn event_log_size_bytes_routes_to_store() {
    let (engine, _store) = make_engine();
    // MemStore inherits the default Store::size_bytes which returns 0,
    // so the engine method should pass that through unchanged.
    assert_eq!(engine.event_log_size_bytes().unwrap(), 0);
}

#[test]
fn take_snapshot_for_bench_delegates_to_take_snapshot() {
    use dp_event::{MemSnapshotStore, SnapshotStore};
    use crate::snapshot::SnapshotConfig;
    let (engine, _) = make_engine();
    let snap_store = MemSnapshotStore::new();
    let seq = crate::take_snapshot_for_bench(&engine, &snap_store, &SnapshotConfig::default(), 0)
        .expect("bench snapshot must succeed");
    assert_eq!(seq, 0);
    assert!(!snap_store.list_seqs().unwrap().is_empty());
}

mod snapshot_recover {
    //! Verifies that `recover()` produces the same projection state when
    //! starting from a snapshot+tail as it does when replaying every
    //! event from seq 0. Drives a deterministic mini-scenario (register
    //! pair → place orders → tick → snapshot mid-flight → more orders +
    //! ticks) on two stores, then compares the public-facing engine
    //! state.

    use std::sync::Arc;
    use std::time::Duration;

    use alloy_primitives::Address;
    use dp_event::{MemSnapshotStore, MemStore, SnapshotStore};
    use dp_types::Side;
    use rust_decimal::Decimal;

    use crate::snapshot::{take_snapshot, SnapshotConfig};
    use crate::test_helpers::{place_plaintext_order, StubAggregator, StubSubmitter};
    use crate::Engine;

    fn dec(n: i64) -> Decimal {
        Decimal::new(n, 0)
    }

    fn wire_engine(store: Arc<MemStore>) -> Engine {
        // Engine::new already installs a NoopDecrypter that parses the
        // JSON-encoded plaintext payload `place_plaintext_order` builds
        // — installing a real decrypter here would lose that round-trip.
        let engine = Engine::new(store, Duration::from_millis(50));
        engine.set_aggregator(Arc::new(StubAggregator::new(vec![0u8; 32])));
        engine.set_submitter(Arc::new(StubSubmitter::new()));
        engine
    }

    async fn drive_scenario(engine: &Engine) {
        engine
            .register_pair_with_event(
                "BTC-USD",
                crate::state::PairConfig::new(Address::repeat_byte(1), Address::repeat_byte(2)),
            )
            .expect("register pair");
        for i in 0..5 {
            place_plaintext_order(
                engine,
                "BTC-USD",
                Side::Buy,
                dec(100),
                dec(1),
                &format!("bid-{i}"),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
            place_plaintext_order(
                engine,
                "BTC-USD",
                Side::Sell,
                dec(100),
                dec(1),
                &format!("ask-{i}"),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        }
        // First tick: matches a few orders, surfaces AuctionExecuted +
        // OrderMatched + BatchSubmitted + BatchConfirmed events.
        engine.run_auction_tick().await;
    }

    fn public_state_fingerprint(engine: &Engine) -> Fingerprint {
        let (bids, asks) = engine.get_order_book("BTC-USD");
        let pairs = engine
            .list_pairs()
            .into_iter()
            .map(|(p, c)| (p, c.status, c.base_token, c.quote_token))
            .collect::<Vec<_>>();
        let auction_log_len = engine
            .get_auction_history(Some("BTC-USD"), 100)
            .map(|v| v.len())
            .unwrap_or(0);
        Fingerprint {
            active_orders: engine.active_order_count(),
            pending_batches: engine.pending_batch_count(),
            bid_ids: bids.iter().map(|o| o.id).collect(),
            ask_ids: asks.iter().map(|o| o.id).collect(),
            pairs,
            auction_log_len,
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Fingerprint {
        active_orders: usize,
        pending_batches: usize,
        bid_ids: Vec<uuid::Uuid>,
        ask_ids: Vec<uuid::Uuid>,
        pairs: Vec<(String, crate::state::PairStatus, Address, Address)>,
        auction_log_len: usize,
    }

    #[tokio::test]
    async fn snapshot_and_replay_yield_identical_state() {
        // Drive the same scenario in two parallel-but-independent worlds:
        // (A) plain event-replay, (B) snapshot + post-snapshot tail replay.
        // The post-tick engines are the "ground truth"; the test then
        // restores fresh engines from the same stores and asserts both
        // recovery paths converge to the same observable state.

        let store_a = Arc::new(MemStore::new());
        let engine_a = wire_engine(store_a.clone());
        drive_scenario(&engine_a).await;
        let ground_truth = public_state_fingerprint(&engine_a);

        let store_b = Arc::new(MemStore::new());
        let snap_store_b: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
        let engine_b = wire_engine(store_b.clone());
        engine_b.set_snapshot_store(Some(snap_store_b.clone()));
        drive_scenario(&engine_b).await;
        // Snapshot at "quiescence" — after the tick + all async finalize
        // calls have returned. take_snapshot uses the engine's current
        // store_last_seq as the watermark cap.
        let now_seq = engine_b.store_last_seq();
        take_snapshot(
            &engine_b,
            snap_store_b.as_ref(),
            &SnapshotConfig::default(),
            now_seq,
        )
        .expect("take snapshot");

        // Apply additional events post-snapshot so the post-snapshot tail
        // is non-empty — exercises the replay-past-snapshot path.
        place_plaintext_order(
            &engine_b,
            "BTC-USD",
            Side::Buy,
            dec(95),
            dec(2),
            "tail-bid",
            Duration::from_secs(60),
        )
        .await
        .unwrap();
        place_plaintext_order(
            &engine_b,
            "BTC-USD",
            Side::Sell,
            dec(95),
            dec(2),
            "tail-ask",
            Duration::from_secs(60),
        )
        .await
        .unwrap();
        engine_b.run_auction_tick().await;
        let tail_truth = public_state_fingerprint(&engine_b);

        // Sanity check: the post-snapshot run should differ from the
        // first scenario's fingerprint (more orders + another tick).
        assert_ne!(
            ground_truth, tail_truth,
            "scenario b should have applied more events than a",
        );

        // Restore from snapshot + replay tail.
        let store_b_restore = store_b.clone();
        let snap_store_b_restore = snap_store_b.clone();
        let engine_b_restored = wire_engine(store_b_restore);
        engine_b_restored.set_snapshot_store(Some(snap_store_b_restore));
        engine_b_restored.recover().await.expect("recover b");
        let restored_fp = public_state_fingerprint(&engine_b_restored);
        assert_eq!(
            restored_fp, tail_truth,
            "snapshot-based recovery must reproduce post-tail state",
        );

        // Restore from full event replay (no snapshot store wired).
        let engine_b_full = wire_engine(store_b.clone());
        // intentionally NO snapshot store, so recover() falls back to
        // event-only replay.
        engine_b_full.recover().await.expect("recover full");
        let full_fp = public_state_fingerprint(&engine_b_full);
        assert_eq!(
            full_fp, tail_truth,
            "full-replay recovery must match the live engine's state",
        );
    }

    #[tokio::test]
    async fn corrupt_snapshot_falls_back_to_full_replay() {
        let store = Arc::new(MemStore::new());
        let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(snap_store.clone()));
        drive_scenario(&engine).await;
        let truth = public_state_fingerprint(&engine);

        // Write a *deliberately garbage* envelope at a sensible seq —
        // recover() should drop it and replay the full event log
        // instead of bricking on the bad bytes.
        snap_store
            .write(engine.store_last_seq(), b"not a real envelope, just trash")
            .unwrap();

        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(snap_store.clone()));
        restored.recover().await.expect("recover with bad snap");
        let restored_fp = public_state_fingerprint(&restored);
        assert_eq!(
            restored_fp, truth,
            "corrupt snapshot must fall back transparently to full replay",
        );
    }

    #[tokio::test]
    async fn corrupt_latest_falls_back_to_older_snapshot() {
        // With `retain_count=3` the snapshotter keeps multiple
        // envelopes. If the latest is corrupt but an older one is
        // intact, recover() must walk the list descending and use the
        // older snapshot rather than redoing a full replay (which would
        // be wrong if the log is compacted).
        let store = Arc::new(MemStore::new());
        let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(snap_store.clone()));
        drive_scenario(&engine).await;
        let truth_after_scenario = public_state_fingerprint(&engine);

        // Snapshot 1: covers the whole scenario. Good.
        let seq1 = engine.store_last_seq();
        take_snapshot(
            &engine,
            snap_store.as_ref(),
            &SnapshotConfig::default(),
            seq1,
        )
        .expect("take snapshot 1");

        // Add a couple of events so the next snapshot covers a strictly
        // greater seq — then plant a corrupt envelope at that seq so
        // read_latest decode fails and the walk falls back to snap 1.
        place_plaintext_order(
            &engine,
            "BTC-USD",
            Side::Buy,
            dec(80),
            dec(1),
            "tail-1",
            Duration::from_secs(60),
        )
        .await
        .unwrap();
        let seq2 = engine.store_last_seq();
        snap_store
            .write(seq2, b"corrupt latest envelope, must be rejected")
            .unwrap();

        // Restore: latest is garbage, but seq1's envelope is intact and
        // the event log past seq1 (just one OrderPlaced) is also intact,
        // so the resulting fingerprint must include that tail order.
        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(snap_store.clone()));
        restored.recover().await.expect("recover with bad latest");

        // Truth includes one extra placed order from the tail.
        let restored_fp = public_state_fingerprint(&restored);
        assert_eq!(
            restored_fp.pairs, truth_after_scenario.pairs,
            "older snapshot must preserve pair registry",
        );
        assert_eq!(
            restored_fp.active_orders,
            truth_after_scenario.active_orders + 1,
            "tail order placed after the corrupt latest must replay on top of the older snapshot",
        );
    }

    #[tokio::test]
    async fn all_corrupt_and_compacted_log_refuses_boot() {
        // The pathological case the hardening is for: every snapshot
        // envelope on disk is bad AND the event log has been compacted
        // past seq 1. Replaying from seq 0 would silently rebuild a
        // partial book; recover() must refuse to boot instead.
        let store = Arc::new(MemStore::new());
        let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(snap_store.clone()));
        drive_scenario(&engine).await;

        // Compact the event log so seq 1 is no longer present.
        let last = engine.store_last_seq();
        engine.compact_events_before(last).expect("compact");

        // Plant a garbage envelope; this is the only snapshot the
        // store has, and it cannot decode.
        snap_store.write(last, b"garbage").unwrap();

        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(snap_store.clone()));
        let err = restored
            .recover()
            .await
            .expect_err("must refuse boot on corrupt-snap + compacted-log");
        assert!(
            matches!(err, crate::EngineError::SnapshotsCorruptAndLogTruncated),
            "expected SnapshotsCorruptAndLogTruncated, got {err:?}",
        );
    }

    /// A SnapshotStore that always errors on `list_seqs`, triggering the
    /// `StoreError` branch of `recover()`.
    struct AlwaysFailSnapshotStore;
    impl dp_event::SnapshotStore for AlwaysFailSnapshotStore {
        fn write(&self, _seq: u64, _envelope: &[u8]) -> Result<(), dp_event::EventError> {
            Ok(())
        }
        fn read_latest(&self) -> Result<Option<(u64, Vec<u8>)>, dp_event::EventError> {
            Err(dp_event::EventError::Io(std::io::Error::other("injected")))
        }
        fn read_at(&self, _seq: u64) -> Result<Option<Vec<u8>>, dp_event::EventError> {
            Ok(None)
        }
        fn list_seqs(&self) -> Result<Vec<u64>, dp_event::EventError> {
            Err(dp_event::EventError::Io(std::io::Error::other("injected")))
        }
        fn delete_before(&self, _before_seq: u64) -> Result<(), dp_event::EventError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn restored_snapshot_with_tail_gap_refuses_boot() {
        // Build an engine, place events, take a snapshot, then compact
        // the first tail event to manufacture a gap between the snapshot
        // watermark and the retained log.
        let store = Arc::new(MemStore::new());
        let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(snap_store.clone()));
        drive_scenario(&engine).await;

        let snap_seq = engine.store_last_seq();
        take_snapshot(&engine, snap_store.as_ref(), &SnapshotConfig::default(), snap_seq)
            .expect("take snapshot");

        // Add two more events (tail) then compact the first tail event,
        // creating a gap: snap_seq+1 is missing, snap_seq+2 is present.
        engine
            .register_pair_with_event(
                "ETH-USDC",
                crate::state::PairConfig::new(
                    alloy_primitives::Address::repeat_byte(0xAA),
                    alloy_primitives::Address::repeat_byte(0xBB),
                ),
            )
            .unwrap();
        engine
            .register_pair_with_event(
                "SOL-USDC",
                crate::state::PairConfig::new(
                    alloy_primitives::Address::repeat_byte(0xCC),
                    alloy_primitives::Address::repeat_byte(0xDD),
                ),
            )
            .unwrap();

        let gap_before = snap_seq + 2; // removes snap_seq+1, keeps snap_seq+2
        engine.compact_events_before(gap_before).expect("compact");

        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(snap_store.clone()));
        let err = restored
            .recover()
            .await
            .expect_err("gap in tail must refuse boot");
        assert!(
            matches!(err, crate::EngineError::SnapshotsCorruptAndLogTruncated),
            "expected SnapshotsCorruptAndLogTruncated, got {err:?}",
        );
    }

    #[tokio::test]
    async fn store_error_with_compacted_log_refuses_boot() {
        // snapshot store always errors; event log has been compacted so
        // full replay from seq 1 is impossible → must refuse.
        let store = Arc::new(MemStore::new());
        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(Arc::new(AlwaysFailSnapshotStore)));
        drive_scenario(&engine).await;

        let last = engine.store_last_seq();
        engine.compact_events_before(last).expect("compact");

        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(Arc::new(AlwaysFailSnapshotStore)));
        let err = restored
            .recover()
            .await
            .expect_err("store error + compacted log must refuse boot");
        assert!(
            matches!(err, crate::EngineError::SnapshotsCorruptAndLogTruncated),
            "expected SnapshotsCorruptAndLogTruncated, got {err:?}",
        );
    }

    #[tokio::test]
    async fn store_error_with_intact_log_falls_back_to_full_replay() {
        // snapshot store always errors; event log starts at seq 1 →
        // full replay is safe, recover() must succeed.
        let store = Arc::new(MemStore::new());
        let engine = wire_engine(store.clone());
        drive_scenario(&engine).await;
        let truth = public_state_fingerprint(&engine);

        let restored = wire_engine(store.clone());
        restored.set_snapshot_store(Some(Arc::new(AlwaysFailSnapshotStore)));
        restored.recover().await.expect("intact log must allow full replay");
        assert_eq!(public_state_fingerprint(&restored), truth);
    }

    #[tokio::test]
    async fn all_corrupt_and_empty_log_refuses_boot() {
        // Pathological variant of the above: the event log is entirely
        // empty (e.g. operator wiped the event store but left the
        // snapshot dir in place). recover() must refuse rather than
        // silently boot the projection-empty state implied by an empty
        // event log — that would contradict the (now-unreadable)
        // snapshot history.
        let store = Arc::new(MemStore::new());
        let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());

        // Plant a garbage envelope at a non-zero seq so list_seqs is
        // non-empty and read_latest decode fails. No events ever land
        // in the store.
        snap_store.write(42, b"garbage").unwrap();

        let engine = wire_engine(store.clone());
        engine.set_snapshot_store(Some(snap_store.clone()));
        let err = engine
            .recover()
            .await
            .expect_err("must refuse boot on corrupt-snap + empty-log");
        assert!(
            matches!(err, crate::EngineError::SnapshotsCorruptAndLogTruncated),
            "expected SnapshotsCorruptAndLogTruncated, got {err:?}",
        );
    }
}

/// Property-based crash-recovery tests. Randomises the shape of the
/// event stream (mix of bid / ask placements, tick cadence, snapshot
/// timing, optional latest-envelope corruption) and asserts the live
/// engine, a snapshot-based recovery, and a full-replay recovery all
/// converge to the same observable state.
///
/// Each case runs through a fresh single-thread tokio runtime — proptest
/// strategies aren't async, so we drive `Engine::recover` and the
/// scenario via `block_on`. Cases are kept small (low order counts,
/// short tail) so a full proptest sweep finishes inside the standard
/// test budget; the fixed-seed deterministic config is documented next
/// to the strategy.
#[cfg(test)]
mod snapshot_recover_prop {
    use std::sync::Arc;
    use std::time::Duration;

    use alloy_primitives::Address;
    use dp_event::{MemSnapshotStore, MemStore, SnapshotStore};
    use dp_types::Side;
    use proptest::prelude::*;
    use rust_decimal::Decimal;

    use crate::snapshot::{take_snapshot, SnapshotConfig};
    use crate::test_helpers::{place_plaintext_order, StubAggregator, StubSubmitter};
    use crate::Engine;

    /// One mutation the property test can apply to the engine.
    #[derive(Debug, Clone)]
    enum Step {
        PlaceBid,
        PlaceAsk,
        Tick,
    }

    /// Wired engine using the plain-text decrypter `place_plaintext_order`
    /// expects.
    fn wire_engine(store: Arc<MemStore>) -> Engine {
        let engine = Engine::new(store, Duration::from_millis(50));
        engine.set_aggregator(Arc::new(StubAggregator::new(vec![0u8; 32])));
        engine.set_submitter(Arc::new(StubSubmitter::new()));
        engine
    }

    async fn register_pair(engine: &Engine) {
        engine
            .register_pair_with_event(
                "BTC-USD",
                crate::state::PairConfig::new(Address::repeat_byte(1), Address::repeat_byte(2)),
            )
            .expect("register pair");
    }

    /// Apply a single [`Step`] to `engine`. `seq` disambiguates the
    /// commitment key so each placement is unique within the scenario.
    async fn apply_step(engine: &Engine, step: &Step, seq: usize) {
        match step {
            Step::PlaceBid => {
                place_plaintext_order(
                    engine,
                    "BTC-USD",
                    Side::Buy,
                    Decimal::new(100, 0),
                    Decimal::new(1, 0),
                    &format!("bid-{seq}"),
                    Duration::from_secs(60),
                )
                .await
                .unwrap();
            }
            Step::PlaceAsk => {
                place_plaintext_order(
                    engine,
                    "BTC-USD",
                    Side::Sell,
                    Decimal::new(100, 0),
                    Decimal::new(1, 0),
                    &format!("ask-{seq}"),
                    Duration::from_secs(60),
                )
                .await
                .unwrap();
            }
            Step::Tick => {
                let _ = engine.run_auction_tick().await;
            }
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Fingerprint {
        active_orders: usize,
        pending_batches: usize,
        bid_ids: Vec<uuid::Uuid>,
        ask_ids: Vec<uuid::Uuid>,
        auction_log_len: usize,
    }

    fn fingerprint(engine: &Engine) -> Fingerprint {
        let (bids, asks) = engine.get_order_book("BTC-USD");
        let auction_log_len = engine
            .get_auction_history(Some("BTC-USD"), 100)
            .map(|v| v.len())
            .unwrap_or(0);
        Fingerprint {
            active_orders: engine.active_order_count(),
            pending_batches: engine.pending_batch_count(),
            bid_ids: bids.iter().map(|o| o.id).collect(),
            ask_ids: asks.iter().map(|o| o.id).collect(),
            auction_log_len,
        }
    }

    fn step_strategy() -> impl Strategy<Value = Step> {
        prop_oneof![
            4 => Just(Step::PlaceBid),
            4 => Just(Step::PlaceAsk),
            1 => Just(Step::Tick),
        ]
    }

    /// Scenario: a stream of steps, a snapshot point (index into the
    /// stream), and a flag indicating whether to corrupt the latest
    /// envelope after snapshotting (forces the descending walk back to
    /// an earlier good envelope or, when none, to a full replay).
    fn scenario_strategy() -> impl Strategy<Value = (Vec<Step>, usize, bool)> {
        prop::collection::vec(step_strategy(), 2..16).prop_flat_map(|steps| {
            let len = steps.len();
            (
                Just(steps),
                // snapshot point must leave at least one step on
                // each side so both pre- and post-snapshot replay
                // paths get exercised.
                1usize..len,
                any::<bool>(),
            )
        })
    }

    proptest! {
        // Keep the case count modest so the full proptest sweep finishes
        // inside the standard cargo test budget. Each case spawns a
        // tokio runtime, places several orders, runs ticks, and exercises
        // two recovery paths — significantly more expensive than a
        // pure-CPU property.
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn recovery_converges_across_random_streams(
            (steps, snap_at, corrupt_latest) in scenario_strategy(),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let outcome = rt.block_on(async move {
                let store = Arc::new(MemStore::new());
                let snap_store: Arc<dyn SnapshotStore> = Arc::new(MemSnapshotStore::new());
                let engine = wire_engine(store.clone());
                engine.set_snapshot_store(Some(snap_store.clone()));
                register_pair(&engine).await;

                // Phase 1: pre-snapshot steps.
                for (i, s) in steps.iter().take(snap_at).enumerate() {
                    apply_step(&engine, s, i).await;
                }

                // Take a good snapshot covering everything so far.
                let good_seq = engine.store_last_seq();
                take_snapshot(
                    &engine,
                    snap_store.as_ref(),
                    &SnapshotConfig::default(),
                    good_seq,
                )
                .expect("take snapshot");

                // Phase 2: post-snapshot steps. The tail replay must
                // pick these up on top of the snapshot state.
                for (j, s) in steps.iter().enumerate().skip(snap_at) {
                    apply_step(&engine, s, j).await;
                }

                // Optionally plant a corrupt envelope at the latest seq
                // so read_latest decode fails and recover() must walk
                // back to the good snapshot. The event tail past `good_seq`
                // is still on disk so the older-snapshot path must
                // produce the same fingerprint as the live engine.
                if corrupt_latest {
                    let latest = engine.store_last_seq();
                    if latest != good_seq {
                        snap_store
                            .write(latest, b"proptest corrupted envelope")
                            .unwrap();
                    }
                }

                let truth = fingerprint(&engine);

                // Recovery A: snapshot + tail (the path we're stressing).
                let restore_a = wire_engine(store.clone());
                restore_a.set_snapshot_store(Some(snap_store.clone()));
                restore_a.recover().await.expect("recover snapshot path");
                let fp_a = fingerprint(&restore_a);

                // Recovery B: full replay (no snapshot store wired).
                let restore_b = wire_engine(store.clone());
                restore_b.recover().await.expect("recover full");
                let fp_b = fingerprint(&restore_b);

                (truth, fp_a, fp_b)
            });

            let (truth, fp_a, fp_b) = outcome;
            prop_assert_eq!(&fp_a, &truth, "snapshot-based recovery diverged from live engine");
            prop_assert_eq!(&fp_b, &truth, "full-replay recovery diverged from live engine");
        }
    }
}
