use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dp_crypto::Decrypter;
use dp_event::{Event, EventData};
use dp_types::{EventType, Order};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::engine::Engine;
use crate::error::EngineError;
use crate::state::{AuctionExecutedRecord, PendingBatch};

const RECOVER_BATCH: usize = 1024;

#[derive(Clone)]
struct OrphanMatch {
    bid: dp_types::Fill,
    ask: dp_types::Fill,
    price: Decimal,
    size: Decimal,
}

impl Engine {
    pub async fn recover(&self) -> Result<(), EngineError> {
        // Fast-path: already recovered.
        {
            let state = self.inner.state.lock();
            if state.recovered {
                return Ok(());
            }
        }

        let decrypter = self.inner.decrypter.read().clone();
        let aggregator = self.inner.aggregator.read().clone();
        let store = self.inner.store.clone();

        let (default_ttl, _) = {
            let mut state = self.inner.state.lock();
            state.reset_projection();
            (state.default_ttl, ())
        };

        // First pass: replay events, decrypting + applying without holding lock
        // across .await. We hold the lock briefly per-event to mutate state.
        let mut after_seq: u64 = 0;
        let mut matches_by_auction: HashMap<Uuid, Vec<OrphanMatch>> = HashMap::new();
        let mut auction_timestamps: HashMap<Uuid, DateTime<Utc>> = HashMap::new();

        loop {
            let events = store.read_from(after_seq, RECOVER_BATCH)?;
            if events.is_empty() {
                break;
            }
            after_seq = events.last().unwrap().seq;

            for ev in events {
                self.replay_event(
                    &ev,
                    decrypter.as_ref(),
                    default_ttl,
                    &mut matches_by_auction,
                    &mut auction_timestamps,
                )
                .await?;
            }
        }

        // Second pass: orphan re-aggregation. Determinism via sorted UUIDs.
        let mut orphan_ids: Vec<Uuid> = matches_by_auction.keys().copied().collect();
        orphan_ids.sort();

        for auction_id in orphan_ids {
            let orphans = matches_by_auction.remove(&auction_id).unwrap();
            let matches: Vec<dp_auction::Match> = orphans
                .iter()
                .map(|m| dp_auction::Match {
                    bid: m.bid.clone(),
                    ask: m.ask.clone(),
                    price: m.price,
                    size: m.size,
                })
                .collect();

            let batch_id = Uuid::new_v4();
            // Orphan secrets wiped on restart → empty witness.
            let witness = dp_zk::witness::BatchWitness::empty(batch_id, auction_id);
            let proof = match aggregator
                .aggregate(batch_id, auction_id, &matches, &witness)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        auction_id = %auction_id,
                        match_count = matches.len(),
                        "orphan re-aggregate failed; settlement event will NOT be replayed: {e}"
                    );
                    continue;
                }
            };

            let ts = auction_timestamps
                .get(&auction_id)
                .copied()
                .unwrap_or_else(Utc::now);

            let mut events = [Event {
                seq: 0,
                event_type: EventType::BatchSubmitted,
                timestamp: ts,
                data: EventData::BatchSubmitted {
                    batch_id,
                    auction_id,
                    tx_hash: String::new(),
                    match_count: matches.len() as u32,
                    proof: proof.clone(),
                },
            }];

            let mut state = self.inner.state.lock();
            self.inner.store.append(&mut events)?;
            state.book.apply(&events[0]);
            state.pending_batches.insert(
                batch_id,
                PendingBatch {
                    batch_id,
                    auction_id,
                    matches: matches.clone(),
                    proof,
                    attempts: 0,
                    next_attempt: None,
                    submitting: false,
                },
            );
        }

        let mut state = self.inner.state.lock();
        state.recovered = true;
        Ok(())
    }

    async fn replay_event(
        &self,
        ev: &Event,
        decrypter: &dyn Decrypter,
        default_ttl: Duration,
        matches_by_auction: &mut HashMap<Uuid, Vec<OrphanMatch>>,
        auction_timestamps: &mut HashMap<Uuid, DateTime<Utc>>,
    ) -> Result<(), EngineError> {
        match &ev.data {
            EventData::OrderPlaced {
                order_id,
                commitment,
                ciphertext,
                salt_nonce,
                ..
            } => {
                let decrypted =
                    decrypter
                        .decrypt(ciphertext)
                        .await
                        .map_err(|source| EngineError::RecoverDecrypt {
                            order_id: *order_id,
                            source,
                        })?;
                let nonce: [u8; 32] = salt_nonce
                    .as_slice()
                    .try_into()
                    .unwrap_or([0u8; 32]);
                let recomputed = crate::engine::recompute_persisted_commitment(
                    *order_id,
                    &decrypted.commitment_key,
                    decrypted.side as u8,
                    decrypted.price,
                    decrypted.size,
                    &nonce,
                );
                if recomputed.as_slice() != commitment.as_slice() {
                    return Err(EngineError::RecoverCommitmentMismatch {
                        order_id: *order_id,
                    });
                }
                let ttl = if decrypted.ttl > 0 {
                    Duration::from_nanos(decrypted.ttl as u64)
                } else {
                    default_ttl
                };
                let expires_at = ev.timestamp
                    + chrono::Duration::from_std(ttl)
                        .unwrap_or_else(|_| chrono::Duration::seconds(600));
                let order = Order {
                    id: *order_id,
                    pair: decrypted.pair.clone(),
                    side: decrypted.side,
                    price: decrypted.price,
                    size: decrypted.size,
                    remaining_size: decrypted.size,
                    commitment_key: decrypted.commitment_key.clone(),
                    encrypted_payload: ciphertext.clone(),
                    submitted_at: ev.timestamp,
                    expires_at,
                };
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                state.book.insert_order(order.clone());
                state.pairs.insert(order.pair);
            }
            EventData::OrderCancelled { .. } | EventData::OrderExpired { .. } => {
                self.inner.state.lock().book.apply(ev);
            }
            EventData::AuctionExecuted {
                auction_id,
                pair,
                clearing_price,
                matched_volume,
                match_count,
                timestamp,
            } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                state.auction_log.push(AuctionExecutedRecord {
                    auction_id: *auction_id,
                    pair: pair.clone(),
                    clearing_price: *clearing_price,
                    matched_volume: *matched_volume,
                    match_count: *match_count,
                    timestamp: *timestamp,
                });
                auction_timestamps.insert(*auction_id, *timestamp);
            }
            EventData::OrderMatched {
                auction_id,
                bid,
                ask,
                price,
                size,
            } => {
                self.inner.state.lock().book.apply(ev);
                matches_by_auction
                    .entry(*auction_id)
                    .or_default()
                    .push(OrphanMatch {
                        bid: bid.clone(),
                        ask: ask.clone(),
                        price: *price,
                        size: *size,
                    });
            }
            EventData::BatchSubmitted {
                batch_id,
                auction_id,
                proof,
                ..
            } => {
                let orphans = matches_by_auction.remove(auction_id).unwrap_or_default();
                let matches: Vec<dp_auction::Match> = orphans
                    .iter()
                    .map(|m| dp_auction::Match {
                        bid: m.bid.clone(),
                        ask: m.ask.clone(),
                        price: m.price,
                        size: m.size,
                    })
                    .collect();
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                state.pending_batches.insert(
                    *batch_id,
                    PendingBatch {
                        batch_id: *batch_id,
                        auction_id: *auction_id,
                        matches,
                        proof: proof.clone(),
                        attempts: 0,
                        next_attempt: None,
                        submitting: false,
                    },
                );
            }
            EventData::BatchConfirmed { batch_id, .. } | EventData::BatchSettled { batch_id, .. } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                state.pending_batches.remove(batch_id);
            }
        }
        Ok(())
    }
}

