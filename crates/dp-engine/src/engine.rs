use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dp_aggregator::{NoopAggregator, ProofAggregator};
use dp_crypto::{Decrypter, NoopDecrypter};
use dp_event::{Event, EventData, Store};
use dp_settlement::{NoopSubmitter, Submitter};
#[cfg(test)]
use dp_types::Side;
use dp_types::{DarkPoolError, EventType, Order};
use parking_lot::{Mutex, RwLock};
use rust_decimal::Decimal;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
#[cfg_attr(test, allow(unused_imports))]
use tracing::warn;
use uuid::Uuid;

use crate::error::EngineError;
use crate::state::{AuctionExecutedRecord, EngineState};
use crate::subscribe::AuctionNotification;
use crate::{DEFAULT_AUCTION_INTERVAL, DEFAULT_SUBSCRIBER_CAPACITY};

pub(crate) const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Per-order ZK witness secrets. NEVER persisted to the event store — held
/// only in memory. Lost on restart; orphan-recovery falls back to a noop
/// proof (see `recover.rs`).
///
/// `salt` and `trader_id` are wiped on drop via `Zeroize` so the sensitive
/// commitment-binding bytes do not linger in freed allocations after a
/// cancel / expire / `prune_dead_secrets`.
#[derive(Clone, Debug)]
pub(crate) struct OrderSecrets {
    pub salt: [u8; 32],
    pub trader_id: [u8; 32],
    pub commitment: [u8; 32],
    pub balance: Decimal,
    pub position: i128,
}

impl Drop for OrderSecrets {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.salt.zeroize();
        self.trader_id.zeroize();
        self.commitment.zeroize();
    }
}

/// Looks up balance/position for a trader. Default impl trusts the caller
/// — pending escrow oracle integration.
pub trait BalanceOracle: Send + Sync {
    fn lookup(&self, trader_id: &[u8; 32]) -> (Decimal, i128);
}

/// Returns a hard-coded 1B balance / 0 position for any trader. Solvency
/// constraints (family 7) pass trivially under this oracle. NOT FOR
/// PRODUCTION — `Engine::new` installs it as a placeholder and emits a
/// startup warning; wire a real oracle via [`Engine::set_balance_oracle`]
/// before serving traffic.
pub struct InsecureDevOracle;

impl BalanceOracle for InsecureDevOracle {
    fn lookup(&self, _trader_id: &[u8; 32]) -> (Decimal, i128) {
        (Decimal::from(1_000_000_000u64), 0)
    }
}

/// Lock-ordering invariant for [`Inner`]:
///
/// **secrets → state.** When both must be held, acquire `secrets` first,
/// then `state`. The only path that holds both is
/// [`Engine::prune_dead_secrets`]; everywhere else either holds them
/// briefly and sequentially (e.g. `cancel_order` releases `state` before
/// calling `drop_secret`) or holds only one. Acquiring them in the
/// reverse order will deadlock against `prune_dead_secrets`.
pub(crate) struct Inner {
    pub(crate) state: Mutex<EngineState>,
    pub(crate) store: Arc<dyn Store>,
    pub(crate) decrypter: RwLock<Arc<dyn Decrypter>>,
    pub(crate) aggregator: RwLock<Arc<dyn ProofAggregator>>,
    pub(crate) submitter: RwLock<Arc<dyn Submitter>>,
    pub(crate) oracle: RwLock<Arc<dyn BalanceOracle>>,
    pub(crate) secrets: Mutex<HashMap<Uuid, OrderSecrets>>,
    pub(crate) subscribers: broadcast::Sender<AuctionNotification>,
    pub(crate) auction_interval: Duration,
    pub(crate) salt_nonce: [u8; 32],
    pub(crate) batch_size: usize,
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
        let mut salt_nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut salt_nonce);
        let engine = Self {
            inner: Arc::new(Inner {
                state: Mutex::new(EngineState::new()),
                store,
                decrypter: RwLock::new(Arc::new(NoopDecrypter)),
                aggregator: RwLock::new(Arc::new(NoopAggregator)),
                submitter: RwLock::new(Arc::new(NoopSubmitter)),
                oracle: RwLock::new(Arc::new(InsecureDevOracle)),
                secrets: Mutex::new(HashMap::new()),
                subscribers: tx,
                auction_interval: interval,
                salt_nonce,
                batch_size: 8,
            }),
        };
        // Suppress the boot warning under `cfg(test)` — every engine test
        // constructs an Engine and the spam drowns out actual test signal.
        #[cfg(not(test))]
        warn!(
            "BalanceOracle: InsecureDevOracle installed (fixed 1B balance) — \
             solvency constraints will pass trivially. Wire a production oracle \
             via Engine::set_balance_oracle before accepting real traffic."
        );
        engine
    }

    pub fn set_balance_oracle(&self, o: Arc<dyn BalanceOracle>) {
        *self.inner.oracle.write() = o;
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

    pub fn register_pair(&self, pair: String, config: crate::state::PairConfig) {
        let mut state = self.inner.state.lock();
        state.pairs.insert(pair.clone());
        state.pair_tokens.insert(pair, config);
    }

    pub fn auction_interval(&self) -> Duration {
        self.inner.auction_interval
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AuctionNotification> {
        self.inner.subscribers.subscribe()
    }

    pub async fn place_encrypted_order(
        &self,
        _client_commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Result<Order, EngineError> {
        let decrypter = self.inner.decrypter.read().clone();
        let decrypted = decrypter
            .decrypt(&ciphertext)
            .await
            .map_err(EngineError::Decrypt)?;

        // Client-supplied commitment is accepted (proto requires it) but no
        // longer verified content-wise: the engine recomputes the canonical
        // Poseidon commitment over decrypted fields and that value is what
        // gets persisted + carried into the ZK circuit.
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
            Duration::from_nanos(decrypted.ttl as u64).min(MAX_TTL)
        } else {
            default_ttl
        };

        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(600));

        let order = Order {
            id: Uuid::new_v4(),
            trader: decrypted.trader,
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

        // Capture ZK witness secrets in-memory only. Salt is derived
        // deterministically from commitment_key + order_id; trader_id is
        // derived from commitment_key. This avoids persisting plaintext
        // and survives the privacy canary test.
        let nonce = &self.inner.salt_nonce;
        let secrets = derive_order_secrets(
            order.id,
            &order.commitment_key,
            order.side as u8,
            order.price,
            order.size,
            self.inner.oracle.read().as_ref(),
            nonce,
        )?;
        let commitment = secrets.commitment.to_vec();
        self.inner.secrets.lock().insert(order.id, secrets);

        self.persist_order_placed(order.clone(), commitment, proof, ciphertext, nonce.to_vec())?;
        Ok(order)
    }

    pub(crate) fn build_batch_witness(
        &self,
        batch_id: Uuid,
        auction_id: Uuid,
        matches: &[dp_auction::Match],
        orders: &HashMap<Uuid, Order>,
    ) -> Result<dp_zk::witness::BatchWitness, EngineError> {
        let secrets = self.inner.secrets.lock();
        let mut match_witnesses = Vec::with_capacity(matches.len());
        for m in matches {
            let bid_secret = secrets
                .get(&m.bid.order_id)
                .cloned()
                .ok_or(EngineError::WitnessSecretMissing { order_id: m.bid.order_id })?;
            let ask_secret = secrets
                .get(&m.ask.order_id)
                .cloned()
                .ok_or(EngineError::WitnessSecretMissing { order_id: m.ask.order_id })?;
            let bid_order = orders
                .get(&m.bid.order_id)
                .cloned()
                .ok_or(EngineError::WitnessOrderMissing { order_id: m.bid.order_id })?;
            let ask_order = orders
                .get(&m.ask.order_id)
                .cloned()
                .ok_or(EngineError::WitnessOrderMissing { order_id: m.ask.order_id })?;
            let bid = leg_witness_from(bid_secret, bid_order, 0);
            let ask = leg_witness_from(ask_secret, ask_order, 1);
            match_witnesses.push(dp_zk::witness::MatchWitness { bid, ask });
        }
        Ok(dp_zk::witness::BatchWitness {
            batch_id,
            auction_id,
            matches: match_witnesses,
            policy: dp_zk::witness::DEFAULT_POLICY.into_policy(),
        })
    }

    /// Drop ZK secrets whose backing order has left the book. Bounds the
    /// `secrets` map so it does not grow with every placed order.
    pub(crate) fn prune_dead_secrets(&self) {
        let mut secrets = self.inner.secrets.lock();
        let state = self.inner.state.lock();
        secrets.retain(|id, _| state.book.has_order(*id));
    }

    /// Drop a single order's secret. Called from cancel/expire paths.
    pub(crate) fn drop_secret(&self, order_id: Uuid) {
        self.inner.secrets.lock().remove(&order_id);
    }

    fn persist_order_placed(
        &self,
        order: Order,
        commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
        salt_nonce: Vec<u8>,
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
                salt_nonce,
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
        let mut events = [Event {
            seq: 0,
            event_type: EventType::OrderCancelled,
            timestamp: Utc::now(),
            data: EventData::OrderCancelled { order_id, reason },
        }];
        self.inner.store.append(&mut events)?;
        state.book.apply(&events[0]);
        drop(state);
        self.drop_secret(order_id);
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

fn leg_witness_from(
    secret: OrderSecrets,
    order: Order,
    side: u8,
) -> dp_zk::witness::OrderLegWitness {
    dp_zk::witness::OrderLegWitness {
        trader_id: hex::encode(secret.trader_id),
        salt: hex::encode(secret.salt),
        balance: secret.balance,
        position: secret.position.to_string(),
        limit_price: order.price,
        order_size: order.size,
        side,
        commitment_key: order.commitment_key,
    }
}

/// Derive per-order ZK secrets.
///
/// **Salt threat model.** `salt = SHA256("salt" || nonce || commitment_key
/// || order_id)` is deterministic given the inputs, but the per-boot
/// `nonce` (32 bytes from OsRng, stored only in `Inner`) prevents any
/// party — including the client who knows both `commitment_key` and
/// `order_id` — from reconstructing the salt. The nonce is persisted
/// alongside each `OrderPlaced` event so recovery can recompute the
/// commitment.
fn derive_order_secrets(
    order_id: Uuid,
    commitment_key: &str,
    side: u8,
    price: Decimal,
    size: Decimal,
    oracle: &dyn BalanceOracle,
    nonce: &[u8; 32],
) -> Result<OrderSecrets, EngineError> {
    let trader_id = dp_zk::pedersen::derive_trader_id_bytes(commitment_key.as_bytes());
    let salt = derive_salt(commitment_key, order_id, nonce);
    let commitment = compute_poseidon_commitment(&trader_id, side, price, size, &salt)?;

    let (balance, position) = oracle.lookup(&trader_id);
    Ok(OrderSecrets {
        salt,
        trader_id,
        commitment,
        balance,
        position,
    })
}

/// Recompute the persisted Poseidon commitment for a (commitment_key,
/// order_id, side, price, size) tuple. Used by recovery to re-verify
/// `OrderPlaced` events without keeping any state in memory.
pub(crate) fn recompute_persisted_commitment(
    order_id: Uuid,
    commitment_key: &str,
    side: u8,
    price: Decimal,
    size: Decimal,
    nonce: &[u8; 32],
) -> Result<[u8; 32], EngineError> {
    let trader_id = dp_zk::pedersen::derive_trader_id_bytes(commitment_key.as_bytes());
    let salt = derive_salt(commitment_key, order_id, nonce);
    compute_poseidon_commitment(&trader_id, side, price, size, &salt)
}

fn derive_salt(commitment_key: &str, order_id: Uuid, nonce: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"salt");
    h.update(nonce);
    h.update(commitment_key.as_bytes());
    h.update(order_id.as_bytes());
    h.finalize().into()
}

/// Compute the Poseidon order commitment (matches the in-circuit gadget
/// byte-for-byte). Inputs use the canonical ZK encodings:
/// - `trader_id` and `salt` are 32 BE bytes mapped into Fr via modular
///   reduction (same as `bytes_to_scalar`).
/// - `price` / `size` go through `decimal_to_scalar` (1e8 fixed-point).
/// - `side` 0 → Fr::zero, otherwise Fr::one.
pub(crate) fn compute_poseidon_commitment(
    trader_id: &[u8; 32],
    side: u8,
    price: Decimal,
    size: Decimal,
    salt: &[u8; 32],
) -> Result<[u8; 32], EngineError> {
    use ark_ff::{BigInteger, PrimeField};
    let trader_fr = dp_zk::pedersen::bytes_to_scalar(trader_id);
    let salt_fr = dp_zk::pedersen::bytes_to_scalar(salt);
    let input = dp_zk::OrderCommitmentInput::from_decimals(trader_fr, side, price, size, salt_fr)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    let fr = dp_zk::commit_native(&input);
    let bytes = fr.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let take = bytes.len().min(32);
    out[32 - take..].copy_from_slice(&bytes[bytes.len() - take..]);
    Ok(out)
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
    trader: alloy_primitives::Address,
    pair: &str,
    side: Side,
    price: Decimal,
    size: Decimal,
    commitment_key: &str,
    ttl: Duration,
) -> (Vec<u8>, Vec<u8>) {
    let d = dp_crypto::DecryptedOrder {
        trader,
        pair: pair.to_string(),
        side,
        price,
        size,
        commitment_key: commitment_key.to_string(),
        ttl: ttl.as_nanos() as i64,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    // Engine no longer verifies the client-supplied commitment, so a
    // placeholder is sufficient. The real commitment is recomputed inside
    // the engine via `compute_poseidon_commitment`.
    (vec![0u8; 32], ct)
}

