use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dp_aggregator::{NoopAggregator, ProofAggregator};
use dp_crypto::{compute_commitment, Decrypter, NoopDecrypter};
use dp_event::{Event, EventData, Store};
use dp_settlement::{NoopSubmitter, Submitter};
#[cfg(test)]
use dp_types::Side;
use dp_types::{DarkPoolError, EventType, Order};
use parking_lot::{Mutex, RwLock};
use rust_decimal::Decimal;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::EngineError;
use crate::state::{AuctionExecutedRecord, EngineState};
use crate::subscribe::AuctionNotification;
use crate::{DEFAULT_AUCTION_INTERVAL, DEFAULT_SUBSCRIBER_CAPACITY};

pub(crate) struct Inner {
    pub(crate) state: Mutex<EngineState>,
    pub(crate) store: Arc<dyn Store>,
    pub(crate) decrypter: RwLock<Arc<dyn Decrypter>>,
    pub(crate) aggregator: RwLock<Arc<dyn ProofAggregator>>,
    pub(crate) submitter: RwLock<Arc<dyn Submitter>>,
    pub(crate) subscribers: broadcast::Sender<AuctionNotification>,
    pub(crate) auction_interval: Duration,
}

#[derive(Clone)]
pub struct Engine {
    pub(crate) inner: Arc<Inner>,
}

impl Engine {
    pub fn new(store: Arc<dyn Store>, auction_interval: Duration) -> Self {
        let interval = if auction_interval.is_zero() {
            DEFAULT_AUCTION_INTERVAL
        } else {
            auction_interval
        };
        let (tx, _rx) = broadcast::channel(DEFAULT_SUBSCRIBER_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(EngineState::new()),
                store,
                decrypter: RwLock::new(Arc::new(NoopDecrypter)),
                aggregator: RwLock::new(Arc::new(NoopAggregator)),
                submitter: RwLock::new(Arc::new(NoopSubmitter)),
                subscribers: tx,
                auction_interval: interval,
            }),
        }
    }

    pub fn set_decrypter(&self, d: Arc<dyn Decrypter>) {
        *self.inner.decrypter.write() = d;
    }

    pub fn set_aggregator(&self, a: Arc<dyn ProofAggregator>) {
        *self.inner.aggregator.write() = a;
    }

    pub fn set_submitter(&self, s: Arc<dyn Submitter>) {
        *self.inner.submitter.write() = s;
    }

    pub fn set_submit_timeout(&self, d: Duration) {
        let mut state = self.inner.state.lock();
        state.submit_timeout = if d.is_zero() {
            crate::DEFAULT_SUBMIT_TIMEOUT
        } else {
            d
        };
    }

    pub fn set_retry_backoff(&self, min: Duration, max: Duration) {
        let max = if max < min { min } else { max };
        let mut state = self.inner.state.lock();
        state.min_backoff = min;
        state.max_backoff = max;
    }

    pub fn auction_interval(&self) -> Duration {
        self.inner.auction_interval
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AuctionNotification> {
        self.inner.subscribers.subscribe()
    }

    pub async fn place_encrypted_order(
        &self,
        commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Result<Order, EngineError> {
        let decrypter = self.inner.decrypter.read().clone();
        let decrypted = decrypter
            .decrypt(&ciphertext)
            .await
            .map_err(EngineError::Decrypt)?;

        if compute_commitment(&decrypted) != commitment {
            return Err(EngineError::Validation(DarkPoolError::CommitmentMismatch));
        }
        if decrypted.pair.is_empty() {
            return Err(EngineError::Validation(DarkPoolError::PairRequired));
        }
        if !decrypted.price.is_sign_positive() || decrypted.price.is_zero() {
            return Err(EngineError::Validation(DarkPoolError::PriceMustBePositive));
        }
        if !decrypted.size.is_sign_positive() || decrypted.size.is_zero() {
            return Err(EngineError::Validation(DarkPoolError::SizeMustBePositive));
        }
        if decrypted.commitment_key.is_empty() {
            return Err(EngineError::Validation(DarkPoolError::CommitmentKeyRequired));
        }

        let default_ttl = self.inner.state.lock().default_ttl;
        let ttl = if decrypted.ttl > 0 {
            Duration::from_nanos(decrypted.ttl as u64)
        } else {
            default_ttl
        };

        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(600));

        let order = Order {
            id: Uuid::new_v4(),
            pair: decrypted.pair.clone(),
            side: decrypted.side,
            price: decrypted.price,
            size: decrypted.size,
            remaining_size: decrypted.size,
            commitment_key: decrypted.commitment_key.clone(),
            encrypted_payload: ciphertext.clone(),
            submitted_at: now,
            expires_at,
        };

        self.persist_order_placed(order.clone(), commitment, proof, ciphertext)?;
        Ok(order)
    }

    fn persist_order_placed(
        &self,
        order: Order,
        commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Result<(), EngineError> {
        let mut events = [Event {
            seq: 0,
            event_type: EventType::OrderPlaced,
            timestamp: order.submitted_at,
            data: EventData::OrderPlaced {
                order_id: order.id,
                commitment,
                proof,
                ciphertext,
            },
        }];

        let mut state = self.inner.state.lock();
        self.inner.store.append(&mut events)?;
        let evt = &events[0];
        state.book.apply(evt);
        state.book.insert_order(order.clone());
        state.pairs.insert(order.pair);
        Ok(())
    }

    pub fn cancel_order(&self, order_id: Uuid, reason: Option<String>) -> Result<(), EngineError> {
        let reason = reason.unwrap_or_else(|| "user cancelled".to_string());
        let state = self.inner.state.lock();
        if !state.book.has_order(order_id) {
            return Err(EngineError::Validation(DarkPoolError::OrderNotFound));
        }
        drop(state);

        let mut events = [Event {
            seq: 0,
            event_type: EventType::OrderCancelled,
            timestamp: Utc::now(),
            data: EventData::OrderCancelled { order_id, reason },
        }];

        let state = self.inner.state.lock();
        self.inner.store.append(&mut events)?;
        state.book.apply(&events[0]);
        Ok(())
    }

    pub fn get_order(&self, order_id: Uuid) -> Option<Order> {
        let state = self.inner.state.lock();
        state.book.find_order(order_id)
    }

    pub fn get_order_book(&self, pair: &str) -> (Vec<Order>, Vec<Order>) {
        let state = self.inner.state.lock();
        let bids: Vec<Order> = state
            .book
            .bids()
            .into_iter()
            .filter(|o| o.pair == pair)
            .collect();
        let asks: Vec<Order> = state
            .book
            .asks()
            .into_iter()
            .filter(|o| o.pair == pair)
            .collect();
        (bids, asks)
    }

    pub fn active_order_count(&self) -> usize {
        let state = self.inner.state.lock();
        state.book.active_order_count()
    }

    pub fn pending_batch_count(&self) -> usize {
        let state = self.inner.state.lock();
        state.pending_batches.len()
    }

    pub fn get_auction_history(
        &self,
        pair: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AuctionNotification>, EngineError> {
        let limit = if limit == 0 { 50 } else { limit };
        let state = self.inner.state.lock();
        let mut out = Vec::new();
        for ae in state.auction_log.iter().rev() {
            if let Some(p) = pair {
                if ae.pair != p {
                    continue;
                }
            }
            out.push(record_to_notification(ae));
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }
}

pub(crate) fn record_to_notification(rec: &AuctionExecutedRecord) -> AuctionNotification {
    AuctionNotification {
        auction_id: rec.auction_id,
        pair: rec.pair.clone(),
        clearing_price: rec.clearing_price,
        matched_volume: rec.matched_volume,
        match_count: rec.match_count,
        timestamp: rec.timestamp,
    }
}

pub(crate) fn order_matched_to_match(
    bid: dp_types::Fill,
    ask: dp_types::Fill,
    price: Decimal,
    size: Decimal,
) -> dp_auction::Match {
    dp_auction::Match {
        bid,
        ask,
        price,
        size,
    }
}

#[cfg(test)]
pub(crate) fn build_decrypted_ciphertext(
    pair: &str,
    side: Side,
    price: Decimal,
    size: Decimal,
    commitment_key: &str,
    ttl: Duration,
) -> (Vec<u8>, Vec<u8>) {
    let d = dp_crypto::DecryptedOrder {
        pair: pair.to_string(),
        side,
        price,
        size,
        commitment_key: commitment_key.to_string(),
        ttl: ttl.as_nanos() as i64,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    let commit = compute_commitment(&d);
    (commit, ct)
}

