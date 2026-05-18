use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use alloy_primitives::U256;
use chrono::Utc;
use dp_aggregator::ProofAggregator;
use dp_event::{Event, EventData};
use dp_settlement::SettlementMatch;
use dp_types::{EventType, Order};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engine::{order_matched_to_match, record_to_notification, Engine};
use crate::state::AuctionExecutedRecord;
use crate::subscribe::AuctionNotification;

#[derive(Clone)]
pub(crate) struct PendingAggregation {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    pub matches: Vec<dp_auction::Match>,
    /// Snapshot of every order referenced by `matches`, captured under the
    /// state lock BEFORE fill events are applied — once `book.apply` runs
    /// for a fully-consumed order it disappears from the book, so we
    /// cannot rely on `book.find_order` later when building the witness.
    pub orders: HashMap<Uuid, Order>,
    pub auction_at: chrono::DateTime<Utc>,
}

impl Engine {
    pub async fn run_auction_tick(&self) -> Vec<AuctionNotification> {
        let (notifications, pending, aggregator) = self.tick_under_lock();

        let mut batches_to_submit = Vec::with_capacity(pending.len());
        for p in pending {
            let witness =
                match self.build_batch_witness(p.batch_id, p.auction_id, &p.matches, &p.orders) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::error!(
                            batch_id = %p.batch_id,
                            auction_id = %p.auction_id,
                            "build witness failed, skipping proof: {e}"
                        );
                        continue;
                    }
                };
            let proof = match aggregator
                .aggregate(p.batch_id, p.auction_id, &p.matches, &witness)
                .await
            {
                Ok(proof) => proof,
                Err(e) => {
                    tracing::warn!(
                        batch_id = %p.batch_id,
                        auction_id = %p.auction_id,
                        "aggregator failed: {e}"
                    );
                    continue;
                }
            };
            let match_prices: Vec<_> = p.matches.iter().map(|m| m.price).collect();
            let match_sizes: Vec<_> = p.matches.iter().map(|m| m.size).collect();
            let public_inputs = match dp_zk::compute_public_inputs(
                &witness,
                &match_prices,
                &match_sizes,
                self.inner.batch_size,
            ) {
                Ok(scalars) => scalars.map(|f| U256::from_be_bytes(dp_zk::fr_to_bytes32(f))),
                Err(e) => {
                    tracing::error!(
                        batch_id = %p.batch_id,
                        "compute public_inputs failed: {e}"
                    );
                    continue;
                }
            };

            let settlement_matches = {
                let state = self.inner.state.lock();
                self.build_settlement_matches(&p, &state)
            };
            let settlement_matches = match settlement_matches {
                Ok(sm) => sm,
                Err(e) => {
                    tracing::error!(batch_id = %p.batch_id, "build settlement_matches: {e}");
                    continue;
                }
            };

            if let Err(e) =
                self.finalize_pending_batch(&p, proof, public_inputs, settlement_matches)
            {
                tracing::warn!(batch_id = %p.batch_id, "finalize batch: {e}");
                continue;
            }
            batches_to_submit.push(p.batch_id);
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
            for evt in expired.iter_mut() {
                evt.timestamp = now;
            }
            if let Err(e) = self.inner.store.append(&mut expired) {
                tracing::warn!("persist expiry events: {e}");
            } else {
                for evt in &expired {
                    state.book.apply(evt);
                }
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
            let result = match dp_auction::run(auction_id, &pair, &bids, &asks) {
                Some(r) => r,
                None => continue,
            };

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
