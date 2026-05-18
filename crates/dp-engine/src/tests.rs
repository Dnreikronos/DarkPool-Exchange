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
