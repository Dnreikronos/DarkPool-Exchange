use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dp_aggregator::{NoopAggregator, ProofAggregator};
use dp_crypto::{Decrypter, NoopDecrypter};
use dp_event::{Event, EventData, EventError, SnapshotStore, Store};
use dp_settlement::{NoopSubmitter, Submitter};
use dp_types::metrics::M_ORDERS_PLACED;
use dp_types::{DarkPoolError, EventType, Order, Side};
use parking_lot::{Mutex, RwLock};
use rand::RngCore;
use rust_decimal::Decimal;
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
    /// Persistent home for periodic state snapshots. `None` disables
    /// the snapshot pipeline — recover then always falls back to full
    /// event replay. Set at boot via [`Engine::set_snapshot_store`].
    pub(crate) snapshot_store: RwLock<Option<Arc<dyn SnapshotStore>>>,
    /// Per-pair IVC round counter: how many fold steps have been
    /// executed for each pair since the last `finalize`.
    pub(crate) ivc_round: Mutex<std::collections::HashMap<String, u64>>,
    /// How many IVC rounds to fold before finalizing and submitting
    /// on-chain. Default: 60.
    pub(crate) finalize_every: std::sync::atomic::AtomicU64,
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
                snapshot_store: RwLock::new(None),
                ivc_round: Mutex::new(std::collections::HashMap::new()),
                finalize_every: std::sync::atomic::AtomicU64::new(60),
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

    /// Override the IVC finalization cadence. The tick loop submits
    /// on-chain every `n` fold steps per pair. Takes effect eventually
    /// on some future tick (Relaxed store; no cross-thread ordering guarantee).
    pub fn set_finalize_every(&self, n: u64) {
        if n == 0 {
            warn!("ignoring finalize_every=0; keeping previous value");
            return;
        }
        self.inner
            .finalize_every
            .store(n, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_submitter(&self, s: Arc<dyn Submitter>) {
        *self.inner.submitter.write() = s;
    }

    /// Wire a persistent [`SnapshotStore`]. Subsequent calls to
    /// [`Engine::recover`] consult it for a starting point before
    /// touching the event log, and the periodic snapshotter task spawned
    /// from `main` writes new envelopes into it. Calling this with
    /// `None` disables snapshot-based recovery without resetting the
    /// in-memory state.
    ///
    /// Note: a `None` set mid-flight does NOT stop a snapshotter task
    /// already running — `run_snapshotter` clones the `Arc` at startup,
    /// so the running task keeps the previous store alive until the
    /// `CancellationToken` fires. Production callers wire the store
    /// once at boot, so this is purely a footgun for tests / integration
    /// fixtures that want to swap stores live.
    pub fn set_snapshot_store(&self, store: Option<Arc<dyn SnapshotStore>>) {
        *self.inner.snapshot_store.write() = store;
    }

    /// Clone of the active [`SnapshotStore`], if one is installed. Used
    /// by the snapshotter task at boot and the recover path when
    /// loading the latest envelope.
    pub fn snapshot_store_clone(&self) -> Option<Arc<dyn SnapshotStore>> {
        self.inner.snapshot_store.read().clone()
    }

    /// Highest event sequence number observed by the underlying event
    /// store. Used by the snapshotter to decide when to checkpoint.
    pub fn store_last_seq(&self) -> u64 {
        self.inner.store.last_seq()
    }

    /// Capture the persistable state under the engine's state mutex.
    /// Returns the cloned subset plus the seq watermark read from the
    /// store under the same lock. Reading `store.last_seq()` under the
    /// state lock ensures the watermark cannot undercount the snapshot:
    /// any append that races with the capture either lands before the
    /// lock (and is included in `snap`) or after (and is excluded), so
    /// `store_seq` always matches exactly what was serialised.
    pub(crate) fn capture_snapshot_state(
        &self,
        _seq_hint: u64,
    ) -> (crate::state::SerializableState, u64) {
        let state = self.inner.state.lock();
        let snap = state.to_serializable();
        // last_seq under the lock — pairing the clone with a watermark
        // outside the lock would race against an `append` landing just
        // after the clone but before we read last_seq.
        let store_seq = self.inner.store.last_seq();
        (snap, store_seq)
    }

    /// Forward a compaction request to the active event store. Pulled
    /// onto `Engine` so the snapshotter does not need a borrow into
    /// `Inner`.
    pub fn compact_events_before(&self, before_seq: u64) -> Result<(), EventError> {
        self.inner.store.compact_before(before_seq)
    }

    /// Replace projection-derived state with `snap`. Locks
    /// `state.book` (write) under the engine mutex; concurrent reads
    /// will block briefly. Caller is responsible for ensuring no other
    /// task is mid-write to the same state (the recover path runs
    /// before the auction tick / API are spawned, which satisfies this
    /// today).
    pub(crate) fn apply_snapshot_state(&self, snap: crate::state::SerializableState) {
        let mut state = self.inner.state.lock();
        state.restore_from_serializable(snap);
    }

    /// Spawn the periodic snapshotter as a background task. Lives in
    /// `main` alongside the auction tick; cancellation comes from the
    /// shared [`tokio_util::sync::CancellationToken`]. A no-op when
    /// `config.enabled` is `false` or no [`SnapshotStore`] is wired.
    pub async fn run_snapshotter(
        &self,
        config: crate::snapshot::SnapshotConfig,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        crate::snapshot::run_snapshotter(self.clone(), config, cancel).await;
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

    /// Insert a pair into the in-memory registry WITHOUT emitting a
    /// `PairRegistered` event. Production code MUST use
    /// [`Engine::register_pair_with_event`] so the registry can be
    /// replayed on restart; this helper exists for tests and integration
    /// fixtures that don't care about the event log. The explicit name
    /// keeps the footgun out of code-review blind spots.
    pub fn register_pair_without_event(&self, pair: String, config: crate::state::PairConfig) {
        let mut state = self.inner.state.lock();
        state.pair_tokens.insert(pair, config);
    }

    /// Register a pair AND persist a `PairRegistered` event. Returns an
    /// error if the pair is already registered or the name fails
    /// [`dp_types::Pair`] canonicalisation. Use this from admin API paths.
    ///
    /// **Blocking note.** The state mutex is held across the
    /// [`Store::append`] call so the in-memory registry and the persisted
    /// event log stay atomically consistent (otherwise a concurrent admin
    /// op could insert a duplicate entry between the existence check and
    /// the append). With a Postgres-backed store this means the engine state lock is
    /// held for one DB round-trip per admin call; trader-path placement,
    /// cancels, orderbook reads, and the auction tick will block briefly.
    /// Admin operations are infrequent so the trade-off favours simplicity
    /// over a reservation-based lock-drop dance.
    pub fn register_pair_with_event(
        &self,
        pair: &str,
        config: crate::state::PairConfig,
    ) -> Result<dp_types::Pair, EngineError> {
        let canonical = dp_types::Pair::parse(pair)?;
        let key = canonical.as_str().to_string();
        let mut events = [Event {
            seq: 0,
            event_type: EventType::PairRegistered,
            timestamp: Utc::now(),
            data: EventData::PairRegistered {
                pair: key.clone(),
                base_token: format!("{:#x}", config.base_token),
                quote_token: format!("{:#x}", config.quote_token),
                min_order_size: config.min_order_size,
                tick_size: config.tick_size,
                auction_interval_ms: config.auction_interval.map(|d| d.as_millis() as u64),
            },
        }];

        let mut state = self.inner.state.lock();
        if state.pair_tokens.contains_key(&key) {
            return Err(EngineError::Validation(
                DarkPoolError::PairAlreadyRegistered(key),
            ));
        }
        self.inner.store.append(&mut events)?;
        state.pair_tokens.insert(key, config);
        Ok(canonical)
    }

    /// Flip a pair's status to `Suspended`. Suspended pairs still appear
    /// in the registry — open orders stay in the book and can be cancelled
    /// but no new orders are accepted and the auction tick skips them.
    ///
    /// Same blocking note as [`Engine::register_pair_with_event`]: the
    /// state mutex is held across the `Store::append` call.
    pub fn suspend_pair(&self, pair: &str) -> Result<(), EngineError> {
        let canonical = dp_types::Pair::parse(pair)?;
        let key = canonical.as_str().to_string();
        let mut events = [Event {
            seq: 0,
            event_type: EventType::PairSuspended,
            timestamp: Utc::now(),
            data: EventData::PairSuspended { pair: key.clone() },
        }];
        let mut state = self.inner.state.lock();
        let cfg = state
            .pair_tokens
            .get_mut(&key)
            .ok_or_else(|| DarkPoolError::PairNotRegistered(key.clone()))?;
        match cfg.status {
            // Suspend is idempotent: skip the event write so a retried
            // admin call doesn't accumulate duplicate `PairSuspended`
            // entries in the log.
            crate::state::PairStatus::Suspended => return Ok(()),
            // Delisted is terminal — cannot demote back to Suspended.
            crate::state::PairStatus::Delisted => {
                return Err(EngineError::Validation(
                    DarkPoolError::CannotSuspendDelistedPair(key),
                ));
            }
            crate::state::PairStatus::Active => {}
        }
        self.inner.store.append(&mut events)?;
        cfg.status = crate::state::PairStatus::Suspended;
        Ok(())
    }

    /// Terminal-state a pair as `Delisted`. Cancels any open orders for
    /// the pair (synchronously emits `OrderCancelled` events) and writes
    /// a `PairDelisted` event. The pair remains in the registry so the
    /// historical record survives replay; trader funds in escrow are
    /// untouched (cancel-without-refund — withdrawal flow handles it).
    ///
    /// Same blocking note as [`Engine::register_pair_with_event`]: the
    /// state mutex is held across the `Store::append` call. Holding the
    /// lock is load-bearing here — otherwise a trader could place a new
    /// order between the open-orders snapshot and the status flip and
    /// survive the delist.
    pub fn delist_pair(&self, pair: &str) -> Result<usize, EngineError> {
        let canonical = dp_types::Pair::parse(pair)?;
        let key = canonical.as_str().to_string();
        let now = Utc::now();

        let mut state = self.inner.state.lock();
        match state.pair_tokens.get(&key).map(|c| c.status) {
            None => {
                return Err(EngineError::Validation(DarkPoolError::PairNotRegistered(
                    key,
                )));
            }
            // Delist is idempotent on the terminal state: skip the event
            // write so retried admin calls don't accumulate duplicate
            // `PairDelisted` entries. Zero orders are cancelled because
            // the previous delist already cancelled them.
            Some(crate::state::PairStatus::Delisted) => return Ok(0),
            Some(_) => {}
        }

        // Collect open orders before we start emitting events. We only
        // need the ids — `book.order_ids_for` avoids cloning every full
        // Order (large `encrypted_payload`s) that `bids()` / `asks()`
        // would, which matters when a delisted pair has many open
        // orders.
        let open_ids = state.book.order_ids_for(&key);

        let mut events: Vec<Event> = Vec::with_capacity(open_ids.len() + 1);
        for id in &open_ids {
            events.push(Event {
                seq: 0,
                event_type: EventType::OrderCancelled,
                timestamp: now,
                data: EventData::OrderCancelled {
                    order_id: *id,
                    reason: "pair delisted".to_string(),
                },
            });
        }
        events.push(Event {
            seq: 0,
            event_type: EventType::PairDelisted,
            timestamp: now,
            data: EventData::PairDelisted { pair: key.clone() },
        });

        self.inner.store.append(&mut events)?;
        for evt in &events {
            state.book.apply(evt);
        }
        if let Some(cfg) = state.pair_tokens.get_mut(&key) {
            cfg.status = crate::state::PairStatus::Delisted;
        }
        drop(state);
        for id in &open_ids {
            self.drop_secret(*id);
        }
        Ok(open_ids.len())
    }

    pub fn list_pairs(&self) -> Vec<(String, crate::state::PairConfig)> {
        let state = self.inner.state.lock();
        state
            .pair_tokens
            .iter()
            .map(|(p, c)| (p.clone(), c.clone()))
            .collect()
    }

    pub fn auction_interval(&self) -> Duration {
        self.inner.auction_interval
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AuctionNotification> {
        self.inner.subscribers.subscribe()
    }

    #[tracing::instrument(
        name = "dp_engine.place_encrypted_order",
        skip(self, _client_commitment, proof, ciphertext),
        fields(
            ciphertext_bytes = ciphertext.len(),
            proof_bytes = proof.len(),
        )
    )]
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
        // Canonicalise the trader-supplied pair so registry lookups and the
        // resulting book entry match the upper-case key the admin path stores
        // (e.g. `eth/usdc` → `ETH/USDC`).
        let canonical_pair = dp_types::Pair::parse(&decrypted.pair)?;
        let pair_key = canonical_pair.into_string();
        if !decrypted.price.is_sign_positive() || decrypted.price.is_zero() {
            return Err(EngineError::Validation(DarkPoolError::PriceMustBePositive));
        }
        if !decrypted.size.is_sign_positive() || decrypted.size.is_zero() {
            return Err(EngineError::Validation(DarkPoolError::SizeMustBePositive));
        }
        if decrypted.commitment_key.is_empty() {
            return Err(EngineError::Validation(
                DarkPoolError::CommitmentKeyRequired,
            ));
        }

        // Pair-registry defense-in-depth. The API layer cannot inspect the
        // encrypted PlaceOrder payload, so the engine is the only point at
        // which an unknown/suspended pair, sub-minimum size, or off-tick
        // price can be rejected.
        {
            let state = self.inner.state.lock();
            let cfg = state.pair_config(&pair_key).ok_or_else(|| {
                EngineError::Validation(DarkPoolError::PairNotRegistered(pair_key.clone()))
            })?;
            match cfg.status {
                crate::state::PairStatus::Active => {}
                crate::state::PairStatus::Suspended | crate::state::PairStatus::Delisted => {
                    return Err(EngineError::Validation(DarkPoolError::PairNotAccepting(
                        pair_key.clone(),
                    )));
                }
            }
            if cfg.min_order_size > Decimal::ZERO && decrypted.size < cfg.min_order_size {
                return Err(EngineError::Validation(
                    DarkPoolError::OrderSizeBelowMinimum {
                        pair: pair_key.clone(),
                        min: cfg.min_order_size.to_string(),
                    },
                ));
            }
            if cfg.tick_size > Decimal::ZERO {
                let rem = decrypted.price % cfg.tick_size;
                if !rem.is_zero() {
                    return Err(EngineError::Validation(
                        DarkPoolError::OrderPriceNotOnTick {
                            pair: pair_key.clone(),
                            tick: cfg.tick_size.to_string(),
                        },
                    ));
                }
            }
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
            pair: pair_key,
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
            let bid_secret =
                secrets
                    .get(&m.bid.order_id)
                    .cloned()
                    .ok_or(EngineError::WitnessSecretMissing {
                        order_id: m.bid.order_id,
                    })?;
            let ask_secret =
                secrets
                    .get(&m.ask.order_id)
                    .cloned()
                    .ok_or(EngineError::WitnessSecretMissing {
                        order_id: m.ask.order_id,
                    })?;
            let bid_order =
                orders
                    .get(&m.bid.order_id)
                    .cloned()
                    .ok_or(EngineError::WitnessOrderMissing {
                        order_id: m.bid.order_id,
                    })?;
            let ask_order =
                orders
                    .get(&m.ask.order_id)
                    .cloned()
                    .ok_or(EngineError::WitnessOrderMissing {
                        order_id: m.ask.order_id,
                    })?;
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

        let state = self.inner.state.lock();
        self.inner.store.append(&mut events)?;
        let evt = &events[0];
        state.book.apply(evt);
        let side_label = match order.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };
        state.book.insert_order(order);
        metrics::counter!(M_ORDERS_PLACED, "side" => side_label).increment(1);
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
        (state.book.bids(pair), state.book.asks(pair))
    }

    pub fn pair_status(&self, pair: &str) -> Option<crate::state::PairStatus> {
        self.inner.state.lock().pair_config(pair).map(|c| c.status)
    }

    pub fn active_order_count(&self) -> usize {
        let state = self.inner.state.lock();
        state.book.active_order_count()
    }

    /// Best-effort size of the persistent event log in bytes. Routed
    /// through the [`Store`] trait so the value is meaningful for every
    /// backend (PgStore queries `pg_total_relation_size`, FileStore
    /// stats the file, MemStore returns 0).
    pub fn event_log_size_bytes(&self) -> Result<u64, dp_event::EventError> {
        self.inner.store.size_bytes()
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
