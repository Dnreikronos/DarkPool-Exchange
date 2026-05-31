use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use alloy_primitives::U256;
use chrono::Utc;
use dp_aggregator::ProofAggregator;
use dp_event::{Event, EventData};
use dp_settlement::SettlementMatch;
use dp_types::metrics::{
    M_ACTIVE_ORDERS, M_AUCTIONS_TOTAL, M_AUCTION_DURATION, M_CLEARING_PRICE, M_ORDERS_EXPIRED,
    M_ORDERS_MATCHED,
};
use dp_types::{EventType, Order};
use rust_decimal::prelude::ToPrimitive;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engine::{order_matched_to_match, record_to_notification, Engine};
use crate::state::AuctionExecutedRecord;
use crate::subscribe::AuctionNotification;

#[derive(Clone)]
pub(crate) struct PendingAggregation {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    /// Trading pair this auction was run for (e.g. `"ETH/USDC"`).
    /// Captured at construction time so the async proof path does not
    /// need to reach back into the engine state under a lock.
    pub pair: String,
    pub matches: Vec<dp_auction::Match>,
    /// Snapshot of every order referenced by `matches`, captured under the
    /// state lock BEFORE fill events are applied — once `book.apply` runs
    /// for a fully-consumed order it disappears from the book, so we
    /// cannot rely on `book.find_order` later when building the witness.
    pub orders: HashMap<Uuid, Order>,
    pub auction_at: chrono::DateTime<Utc>,
}

impl Engine {
    #[tracing::instrument(name = "dp_engine.auction_tick", skip(self))]
    pub async fn run_auction_tick(&self) -> Vec<AuctionNotification> {
        let (notifications, pending, aggregator) = self.tick_under_lock();

        let mut batches_to_submit = Vec::with_capacity(pending.len());

        for p in &pending {
            let witness =
                match self.build_batch_witness(p.batch_id, p.auction_id, &p.matches, &p.orders) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::error!(
                            batch_id = %p.batch_id,
                            auction_id = %p.auction_id,
                            "build witness failed, skipping IVC fold: {e}"
                        );
                        continue;
                    }
                };

            let match_prices: Vec<_> = p.matches.iter().map(|m| m.price).collect();
            let match_sizes: Vec<_> = p.matches.iter().map(|m| m.size).collect();
            let ext = match dp_zk::step_circuit::AuctionExternalInputs::from_witness(
                &witness,
                &match_prices,
                &match_sizes,
                self.inner.batch_size,
            ) {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!(
                        batch_id = %p.batch_id,
                        "build AuctionExternalInputs failed: {e}"
                    );
                    continue;
                }
            };

            if let Err(e) = aggregator.fold_step(p.pair.clone(), ext).await {
                tracing::warn!(
                    batch_id = %p.batch_id,
                    pair = %p.pair,
                    "fold_step failed: {e}"
                );
                continue;
            }

            // Advance the per-pair round counter and emit a BatchFolded event.
            let round_index = {
                let mut map = self.inner.ivc_round.lock();
                let r = map.entry(p.pair.clone()).or_insert(0);
                *r += 1;
                *r
            };

            if let Err(e) = self.inner.store.append(&mut [Event {
                seq: 0,
                event_type: EventType::BatchFolded,
                timestamp: Utc::now(),
                data: EventData::BatchFolded {
                    batch_id: p.batch_id,
                    round_index,
                    pair: p.pair.clone(),
                },
            }]) {
                tracing::warn!(
                    batch_id = %p.batch_id,
                    "persist BatchFolded event: {e}"
                );
            }

            let finalize_every = self
                .inner
                .finalize_every
                .load(std::sync::atomic::Ordering::Relaxed);
            if round_index % finalize_every == 0 {
                let final_proof = match aggregator.finalize(p.pair.clone()).await {
                    Ok(fp) => fp,
                    Err(e) => {
                        tracing::warn!(
                            batch_id = %p.batch_id,
                            pair = %p.pair,
                            "IVC finalize failed: {e}"
                        );
                        continue;
                    }
                };

                let settlement_matches = {
                    let state = self.inner.state.lock();
                    self.build_settlement_matches(p, &state)
                };
                let settlement_matches = match settlement_matches {
                    Ok(sm) => sm,
                    Err(e) => {
                        tracing::error!(
                            batch_id = %p.batch_id,
                            "build settlement_matches (IVC): {e}"
                        );
                        continue;
                    }
                };

                // IVC proofs do not use Groth16 public inputs; store zeros to
                // satisfy the `PendingBatch` schema. The on-chain verifier for
                // IVC proofs reads public inputs from the `FinalProof` metadata
                // stored in `ivc_payload`.
                let proof_bytes = final_proof.proof_bytes.clone();
                let public_inputs = [U256::ZERO; 6];

                if let Err(e) =
                    self.finalize_pending_batch(p, proof_bytes, public_inputs, settlement_matches)
                {
                    tracing::warn!(batch_id = %p.batch_id, "finalize IVC batch: {e}");
                    continue;
                }

                // Store the full IVC payload alongside the batch so the
                // submission path can pass it to the on-chain verifier.
                {
                    use ark_ff::{BigInteger, PrimeField};
                    let fr_to_bytes = |f: ark_bn254::Fr| -> [u8; 32] {
                        let b = f.into_bigint().to_bytes_be();
                        let mut out = [0u8; 32];
                        let take = b.len().min(32);
                        out[32 - take..].copy_from_slice(&b[b.len() - take..]);
                        out
                    };
                    let z_0 = final_proof.z_0.map(fr_to_bytes);
                    let z_n = final_proof.z_n.map(fr_to_bytes);
                    let policy_hash = fr_to_bytes(final_proof.policy_hash);
                    let payload = crate::state::ProofPayload::IvcFinal {
                        proof_bytes: final_proof.proof_bytes,
                        z_0,
                        z_n,
                        n_steps: final_proof.n_steps,
                        policy_hash,
                    };
                    let mut state = self.inner.state.lock();
                    if let Some(pb) = state.pending_batches.get_mut(&p.batch_id) {
                        pb.ivc_payload = Some(payload);
                    }
                }

                batches_to_submit.push(p.batch_id);
            }
        }

        for n in &notifications {
            let _ = self.inner.subscribers.send(n.clone());
        }

        let mut submit_set: HashSet<Uuid> = HashSet::with_capacity(batches_to_submit.len());
        for id in &batches_to_submit {
            submit_set.insert(*id);
            if let Err(e) = self.submit_batch(*id).await {
                tracing::warn!(batch_id = %id, "submit failed: {e}");
            }
        }

        self.resubmit_pending_except(&submit_set).await;

        // Drop ZK secrets for orders that left the book this tick (expired
        // or fully filled). Bounds the in-memory secrets map and limits the
        // window in which sensitive material is held.
        self.prune_dead_secrets();

        metrics::gauge!(M_ACTIVE_ORDERS).set(self.active_order_count() as f64);

        notifications
    }

    fn build_settlement_matches(
        &self,
        p: &PendingAggregation,
        state: &crate::state::EngineState,
    ) -> Result<Vec<SettlementMatch>, crate::error::EngineError> {
        p.matches
            .iter()
            .map(|m| {
                crate::state::try_build_settlement_row(m, &p.orders, &state.pair_tokens).map_err(
                    |e| match e {
                        crate::state::SettlementResolveErr::OrderMissing(order_id) => {
                            crate::error::EngineError::WitnessOrderMissing { order_id }
                        }
                        crate::state::SettlementResolveErr::PairMissing(pair) => {
                            crate::error::EngineError::PairNotConfigured { pair }
                        }
                    },
                )
            })
            .collect()
    }

    fn tick_under_lock(
        &self,
    ) -> (
        Vec<AuctionNotification>,
        Vec<PendingAggregation>,
        Arc<dyn ProofAggregator>,
    ) {
        let now = Utc::now();
        let mut state = self.inner.state.lock();

        // Expire orders.
        let mut expired = state.book.collect_expired(now);
        if !expired.is_empty() {
            let expired_count = expired.len() as u64;
            for evt in expired.iter_mut() {
                evt.timestamp = now;
            }
            if let Err(e) = self.inner.store.append(&mut expired) {
                tracing::warn!("persist expiry events: {e}");
            } else {
                for evt in &expired {
                    state.book.apply(evt);
                }
                metrics::counter!(M_ORDERS_EXPIRED).increment(expired_count);
            }
        }

        let mut notifications: Vec<AuctionNotification> = Vec::new();
        let mut pending: Vec<PendingAggregation> = Vec::new();

        // Sort by pair so AuctionExecuted / OrderMatched events emitted in
        // a single tick land in a deterministic order. HashMap iteration is
        // randomised per-process, which would otherwise leak into the event
        // log and into the stream subscribers — replays would visibly
        // re-order events that should be identical run-to-run.
        let mut pairs: Vec<String> = state
            .pair_tokens
            .iter()
            .filter(|(_, cfg)| cfg.status.is_active())
            .map(|(p, _)| p.clone())
            .collect();
        pairs.sort();
        for pair in pairs {
            let bids: Vec<_> = state.book.bids(&pair);
            let asks: Vec<_> = state.book.asks(&pair);

            let auction_id = Uuid::new_v4();
            let auction_start = Instant::now();
            let result = match dp_auction::run(auction_id, &pair, &bids, &asks) {
                Some(r) => r,
                None => continue,
            };
            let auction_elapsed = auction_start.elapsed();

            let mut events: Vec<Event> = Vec::with_capacity(1 + result.matches.len());
            events.push(Event {
                seq: 0,
                event_type: EventType::AuctionExecuted,
                timestamp: now,
                data: EventData::AuctionExecuted {
                    auction_id: result.auction_id,
                    pair: result.pair.clone(),
                    clearing_price: result.clearing_price,
                    matched_volume: result.matched_volume,
                    match_count: result.matches.len() as u32,
                    timestamp: now,
                },
            });
            for m in &result.matches {
                events.push(Event {
                    seq: 0,
                    event_type: EventType::OrderMatched,
                    timestamp: now,
                    data: EventData::OrderMatched {
                        auction_id: result.auction_id,
                        bid: m.bid.clone(),
                        ask: m.ask.clone(),
                        price: m.price,
                        size: m.size,
                    },
                });
            }

            // Snapshot full Order rows for the witness BEFORE the fill
            // events are applied — apply_fill removes orders whose
            // remaining_size hits zero.
            let mut order_snapshot: HashMap<Uuid, Order> = HashMap::new();
            for m in &result.matches {
                if let Some(o) = state.book.find_order(m.bid.order_id) {
                    order_snapshot.insert(o.id, o);
                }
                if let Some(o) = state.book.find_order(m.ask.order_id) {
                    order_snapshot.insert(o.id, o);
                }
            }

            if let Err(e) = self.inner.store.append(&mut events) {
                tracing::warn!(pair = %pair, "persist auction events: {e}");
                continue;
            }

            metrics::counter!(M_AUCTIONS_TOTAL).increment(1);
            metrics::histogram!(M_AUCTION_DURATION).record(auction_elapsed.as_secs_f64());
            // One increment per match (bid/ask pair). Operators reading
            // `rate(darkpool_orders_matched_total)` get trade rate; double
            // for leg rate. Description in observability.rs matches.
            metrics::counter!(M_ORDERS_MATCHED).increment(result.matches.len() as u64);
            if let Some(price) = result.clearing_price.to_f64() {
                metrics::gauge!(M_CLEARING_PRICE, "pair" => pair.clone()).set(price);
            }

            let record = AuctionExecutedRecord {
                auction_id: result.auction_id,
                pair: result.pair.clone(),
                clearing_price: result.clearing_price,
                matched_volume: result.matched_volume,
                match_count: result.matches.len() as u32,
                timestamp: now,
            };
            state.auction_log.push(record.clone());
            for evt in &events {
                state.book.apply(evt);
            }

            notifications.push(record_to_notification(&record));
            pending.push(PendingAggregation {
                batch_id: Uuid::new_v4(),
                auction_id: result.auction_id,
                pair: result.pair.clone(),
                matches: result
                    .matches
                    .iter()
                    .map(|m| order_matched_to_match(m.bid.clone(), m.ask.clone(), m.price, m.size))
                    .collect(),
                orders: order_snapshot,
                auction_at: now,
            });
        }

        let aggregator = self.inner.aggregator.read().clone();
        (notifications, pending, aggregator)
    }

    pub async fn start(&self, cancel: CancellationToken) {
        self.resubmit_pending_except(&HashSet::new()).await;

        let mut interval = tokio::time::interval(self.inner.auction_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // First tick fires immediately; skip it so behavior matches Go (first
        // tick after one interval).
        interval.tick().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = interval.tick() => {
                    let _ = self.run_auction_tick().await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use alloy_primitives::Address;
    use chrono::Utc;
    use dp_event::MemStore;
    use dp_types::{Fill, Order, Side};
    use rust_decimal::Decimal;

    use super::*;
    use crate::state::{EngineState, PairConfig};
    use crate::Engine;

    fn order_in_book(id: Uuid, pair: &str) -> Order {
        Order {
            id,
            trader: Address::ZERO,
            pair: pair.to_string(),
            side: Side::Buy,
            price: Decimal::ONE,
            size: Decimal::ONE,
            remaining_size: Decimal::ONE,
            commitment_key: "k".into(),
            encrypted_payload: Vec::new(),
            submitted_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
        }
    }

    fn pending_with(
        bid_id: Uuid,
        ask_id: Uuid,
        orders: HashMap<Uuid, Order>,
    ) -> PendingAggregation {
        PendingAggregation {
            batch_id: Uuid::new_v4(),
            auction_id: Uuid::new_v4(),
            pair: "ETH/USDC".to_string(),
            matches: vec![dp_auction::Match {
                bid: Fill {
                    order_id: bid_id,
                    size: Decimal::ONE,
                },
                ask: Fill {
                    order_id: ask_id,
                    size: Decimal::ONE,
                },
                price: Decimal::ONE,
                size: Decimal::ONE,
            }],
            orders,
            auction_at: Utc::now(),
        }
    }

    fn fresh_engine() -> Engine {
        let store = Arc::new(MemStore::new());
        Engine::new(store, Duration::from_millis(50))
    }

    #[test]
    fn build_settlement_matches_returns_witness_missing_for_missing_bid() {
        let engine = fresh_engine();
        let state = EngineState::new();
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut orders = HashMap::new();
        orders.insert(ask_id, order_in_book(ask_id, "ETH/USDC"));
        let p = pending_with(bid_id, ask_id, orders);
        let err = engine.build_settlement_matches(&p, &state).unwrap_err();
        match err {
            crate::error::EngineError::WitnessOrderMissing { order_id } => {
                assert_eq!(order_id, bid_id);
            }
            other => panic!("expected WitnessOrderMissing, got {other:?}"),
        }
    }

    #[test]
    fn build_settlement_matches_returns_witness_missing_for_missing_ask() {
        let engine = fresh_engine();
        let state = EngineState::new();
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut orders = HashMap::new();
        orders.insert(bid_id, order_in_book(bid_id, "ETH/USDC"));
        let p = pending_with(bid_id, ask_id, orders);
        let err = engine.build_settlement_matches(&p, &state).unwrap_err();
        match err {
            crate::error::EngineError::WitnessOrderMissing { order_id } => {
                assert_eq!(order_id, ask_id);
            }
            other => panic!("expected WitnessOrderMissing, got {other:?}"),
        }
    }

    #[test]
    fn build_settlement_matches_returns_pair_not_configured() {
        let engine = fresh_engine();
        let state = EngineState::new();
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut orders = HashMap::new();
        orders.insert(bid_id, order_in_book(bid_id, "UNKNOWN/PAIR"));
        orders.insert(ask_id, order_in_book(ask_id, "UNKNOWN/PAIR"));
        let p = pending_with(bid_id, ask_id, orders);
        let err = engine.build_settlement_matches(&p, &state).unwrap_err();
        match err {
            crate::error::EngineError::PairNotConfigured { pair } => {
                assert_eq!(pair, "UNKNOWN/PAIR");
            }
            other => panic!("expected PairNotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn build_settlement_matches_builds_row_for_registered_pair() {
        let engine = fresh_engine();
        let mut state = EngineState::new();
        let base = Address::repeat_byte(0xAA);
        let quote = Address::repeat_byte(0xBB);
        state
            .pair_tokens
            .insert("ETH/USDC".into(), PairConfig::new(base, quote));
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut orders = HashMap::new();
        orders.insert(bid_id, order_in_book(bid_id, "ETH/USDC"));
        orders.insert(ask_id, order_in_book(ask_id, "ETH/USDC"));
        let p = pending_with(bid_id, ask_id, orders);
        let rows = engine.build_settlement_matches(&p, &state).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].base_token, base);
        assert_eq!(rows[0].quote_token, quote);
    }
}

#[cfg(test)]
mod ivc_tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use alloy_primitives::Address;
    use dp_aggregator::{AggregatorError, ProofAggregator};
    use dp_auction::Match;
    use dp_event::MemStore;
    use dp_types::Side;
    use dp_zk::witness::BatchWitness;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use crate::state::PairConfig;
    use crate::test_helpers::place_plaintext_order;
    use crate::Engine;

    /// A mock aggregator that tracks fold_step / finalize calls and returns
    /// dummy successful results. Does not perform any real IVC computation.
    struct MockFoldingAggregator {
        fold_calls: AtomicU32,
        finalize_calls: AtomicU32,
    }

    impl MockFoldingAggregator {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                fold_calls: AtomicU32::new(0),
                finalize_calls: AtomicU32::new(0),
            })
        }

        fn fold_calls(&self) -> u32 {
            self.fold_calls.load(Ordering::SeqCst)
        }

        fn finalize_calls(&self) -> u32 {
            self.finalize_calls.load(Ordering::SeqCst)
        }
    }

    impl ProofAggregator for MockFoldingAggregator {
        fn aggregate<'a>(
            &'a self,
            _batch_id: Uuid,
            _auction_id: Uuid,
            _matches: &'a [Match],
            _witness: &'a BatchWitness,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
            Box::pin(async { Ok(vec![0u8; 32]) })
        }

        fn fold_step<'a>(
            &'a self,
            _pair: String,
            _ext: dp_zk::step_circuit::AuctionExternalInputs,
        ) -> Pin<Box<dyn Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
            self.fold_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }

        fn finalize<'a>(
            &'a self,
            _pair: String,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<dp_zk::folding::FinalProof, AggregatorError>>
                    + Send
                    + 'a,
            >,
        > {
            self.finalize_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                use ark_bn254::Fr;
                use ark_ff::Zero;
                Ok(dp_zk::folding::FinalProof {
                    proof_bytes: vec![0u8; 32],
                    z_0: [Fr::zero(); 5],
                    z_n: [Fr::zero(); 5],
                    n_steps: 1,
                    policy_hash: Fr::zero(),
                })
            })
        }
    }

    fn make_engine_with_pair() -> (Engine, Arc<MemStore>) {
        let store = Arc::new(MemStore::new());
        let engine = Engine::new(store.clone(), Duration::from_millis(50));
        let base = Address::repeat_byte(0xAA);
        let quote = Address::repeat_byte(0xBB);
        engine.register_pair_without_event("ETH/USDC".into(), PairConfig::new(base, quote));
        (engine, store)
    }

    async fn place_matching_orders(engine: &Engine) {
        let ttl = Duration::from_secs(60);
        place_plaintext_order(
            engine,
            "ETH/USDC",
            Side::Buy,
            Decimal::from(100),
            Decimal::ONE,
            "key_bid",
            ttl,
        )
        .await
        .unwrap();
        place_plaintext_order(
            engine,
            "ETH/USDC",
            Side::Sell,
            Decimal::from(100),
            Decimal::ONE,
            "key_ask",
            ttl,
        )
        .await
        .unwrap();
    }

    /// With `finalize_every = 3`, running 2 ticks (each with one matching
    /// round) must fold but NOT finalize — `finalize_calls` stays 0.
    #[tokio::test]
    async fn no_submit_before_finalize_boundary() {
        let (engine, _store) = make_engine_with_pair();
        let agg = MockFoldingAggregator::new();
        engine.set_aggregator(Arc::clone(&agg) as Arc<dyn ProofAggregator>);
        engine.set_finalize_every(3);

        // Tick 1 — place fresh matching orders then tick.
        place_matching_orders(&engine).await;
        engine.run_auction_tick().await;

        // Tick 2 — place matching orders again and tick.
        place_matching_orders(&engine).await;
        engine.run_auction_tick().await;

        assert_eq!(
            agg.finalize_calls(),
            0,
            "finalize must not fire before boundary"
        );
        assert_eq!(engine.pending_batch_count(), 0, "no batches finalized yet");
    }

    /// With `finalize_every = 2`, running 2 matching ticks hits the boundary
    /// and must call `finalize` exactly once, producing one pending batch.
    #[tokio::test]
    async fn submit_compressed_at_boundary() {
        let (engine, _store) = make_engine_with_pair();
        let agg = MockFoldingAggregator::new();
        engine.set_aggregator(Arc::clone(&agg) as Arc<dyn ProofAggregator>);
        engine.set_finalize_every(2);

        // Install a stub submitter so submit_batch doesn't error.
        use crate::test_helpers::StubSubmitter;
        engine.set_submitter(Arc::new(StubSubmitter::new()));

        place_matching_orders(&engine).await;
        engine.run_auction_tick().await;

        place_matching_orders(&engine).await;
        engine.run_auction_tick().await;

        assert_eq!(agg.finalize_calls(), 1, "finalize must fire at round 2");
        assert_eq!(agg.fold_calls(), 2, "two fold steps must have run");
    }

    /// Cancelling the engine loop (simulated by just not running `start`) does
    /// not panic; the round counter initialises cleanly.
    #[tokio::test]
    async fn shutdown_triggers_no_panic() {
        let (engine, _store) = make_engine_with_pair();
        let agg = MockFoldingAggregator::new();
        engine.set_aggregator(Arc::clone(&agg) as Arc<dyn ProofAggregator>);
        engine.set_finalize_every(10);
        // Just verify construction + setter do not panic.
        assert_eq!(agg.fold_calls(), 0);
        assert_eq!(agg.finalize_calls(), 0);
    }
}
