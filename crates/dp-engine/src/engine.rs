use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dp_aggregator::{NoopAggregator, ProofAggregator};
use dp_crypto::{Decrypter, NoopDecrypter, SnapshotCipher};
use dp_event::{Event, EventData, EventError, SnapshotStore, Store};
use dp_settlement::{BalanceOracle, InsecureDevOracle, NoopSubmitter, Submitter};
use dp_types::metrics::M_ORDERS_PLACED;
use dp_types::{DarkPoolError, EventType, Order, Side};
use parking_lot::{Mutex, RwLock};
#[cfg(test)]
use rand::RngCore;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
#[cfg_attr(test, allow(unused_imports))]
use tracing::warn;
use uuid::Uuid;

use crate::error::EngineError;
use crate::order_proof::OrderProofVerifier;
use crate::state::{AuctionExecutedRecord, EngineState};
use crate::subscribe::AuctionNotification;
use crate::{DEFAULT_AUCTION_INTERVAL, DEFAULT_SUBSCRIBER_CAPACITY};

pub(crate) const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const MAX_ENCRYPTED_PAYLOAD_BYTES: usize = 128 * 1024;
pub const MAX_ACTIVE_ORDERS_PER_TRADER: usize = 128;
pub const MAX_ACTIVE_ORDERS: usize = 16 * 1024;

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
    pub nullifier: [u8; 32],
    pub balance: Decimal,
    pub position: i128,
}

impl Drop for OrderSecrets {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.salt.zeroize();
        self.trader_id.zeroize();
        self.commitment.zeroize();
        self.nullifier.zeroize();
    }
}

/// Lock-ordering invariant for [`Inner`]:
///
/// **secrets → state.** When both must be held, acquire `secrets` first,
/// then `state`. The paths that hold both are [`Engine::prune_dead_secrets`]
/// and the atomic order-admission insert in [`Engine::persist_order_placed`].
/// Everywhere else either holds them briefly and sequentially (e.g.
/// `cancel_order` releases `state` before calling `drop_secret`) or holds only
/// one. Acquiring them in the reverse order will deadlock against
/// `prune_dead_secrets`.
pub(crate) struct Inner {
    pub(crate) state: Mutex<EngineState>,
    pub(crate) store: Arc<dyn Store>,
    pub(crate) decrypter: RwLock<Arc<dyn Decrypter>>,
    pub(crate) aggregator: RwLock<Arc<dyn ProofAggregator>>,
    pub(crate) submitter: RwLock<Arc<dyn Submitter>>,
    pub(crate) oracle: RwLock<Arc<dyn BalanceOracle>>,
    pub(crate) order_proof_verifier: RwLock<Option<Arc<dyn OrderProofVerifier>>>,
    pub(crate) secrets: Mutex<HashMap<Uuid, OrderSecrets>>,
    pub(crate) subscribers: broadcast::Sender<AuctionNotification>,
    pub(crate) auction_interval: Duration,
    pub(crate) batch_size: usize,
    /// Persistent home for periodic state snapshots. `None` disables
    /// the snapshot pipeline — recover then always falls back to full
    /// event replay. Set at boot via [`Engine::set_snapshot_store`].
    pub(crate) snapshot_store: RwLock<Option<Arc<dyn SnapshotStore>>>,
    /// AEAD cipher that seals every snapshot envelope at rest (#203). Set at
    /// boot via [`Engine::set_snapshot_cipher`]. When a `snapshot_store` is
    /// installed this MUST be `Some`, or the snapshotter refuses to run rather
    /// than write plaintext; the boot path (`dp-api`) enforces that policy.
    pub(crate) snapshot_cipher: RwLock<Option<Arc<SnapshotCipher>>>,
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
        let engine = Self {
            inner: Arc::new(Inner {
                state: Mutex::new(EngineState::new()),
                store,
                decrypter: RwLock::new(Arc::new(NoopDecrypter)),
                aggregator: RwLock::new(Arc::new(NoopAggregator)),
                submitter: RwLock::new(Arc::new(NoopSubmitter)),
                oracle: RwLock::new(Arc::new(InsecureDevOracle)),
                order_proof_verifier: RwLock::new(None),
                secrets: Mutex::new(HashMap::new()),
                subscribers: tx,
                auction_interval: interval,
                batch_size: 8,
                snapshot_store: RwLock::new(None),
                snapshot_cipher: RwLock::new(None),
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

    pub fn set_order_proof_verifier(&self, v: Arc<dyn OrderProofVerifier>) {
        *self.inner.order_proof_verifier.write() = Some(v);
    }

    pub fn clear_order_proof_verifier(&self) {
        *self.inner.order_proof_verifier.write() = None;
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

    /// Install the AEAD cipher used to seal/open snapshot envelopes at rest.
    /// Must be set whenever a [`SnapshotStore`] is wired — the snapshotter
    /// fails closed (writes nothing) without it, and recovery cannot decode
    /// existing envelopes. See `Inner::snapshot_cipher`.
    pub fn set_snapshot_cipher(&self, cipher: Option<Arc<SnapshotCipher>>) {
        *self.inner.snapshot_cipher.write() = cipher;
    }

    /// Clone of the active snapshot [`SnapshotCipher`], if one is installed.
    /// Consulted by `crate::snapshot::take_snapshot` before writing and by
    /// the recover path before decoding an envelope.
    pub fn snapshot_cipher_clone(&self) -> Option<Arc<SnapshotCipher>> {
        self.inner.snapshot_cipher.read().clone()
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
                base_decimals: config.base_decimals,
                quote_decimals: config.quote_decimals,
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
    /// lock is load-bearing here — the open-orders snapshot, the cancel
    /// events, and the status flip are one atomic critical section. The
    /// other half of the invariant lives in `persist_order_placed`, which
    /// re-checks the pair status under this same lock before inserting
    /// (issue #162): an order that passed its earlier status check but only
    /// reaches the insert after this delist commits is rejected, so it can
    /// never land in the book after the snapshot that would have cancelled
    /// it.
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

    /// Decrypt, validate, and book an order.
    ///
    /// ## What is cryptographically enforced (#217)
    ///
    /// The operator decrypts the ciphertext and re-derives the canonical
    /// Poseidon commitment plus nullifier from the decrypted fields and
    /// client-chosen salt. When a canonical order-proof verifier is installed
    /// (the production server requires one at boot), the supplied Groth16 proof
    /// must verify against those engine-derived public inputs. The nullifier is
    /// also checked under the same lock that appends `OrderPlaced`, so a proof
    /// replay cannot race two admissions through the book.
    #[tracing::instrument(
        name = "dp_engine.place_encrypted_order",
        skip(self, _client_commitment, _unverified_proof, ciphertext, caller),
        fields(
            ciphertext_bytes = ciphertext.len(),
            order_proof_bytes = _unverified_proof.len(),
        )
    )]
    pub async fn place_encrypted_order(
        &self,
        _client_commitment: Vec<u8>,
        _unverified_proof: Vec<u8>,
        ciphertext: Vec<u8>,
        caller: Option<alloy_primitives::Address>,
    ) -> Result<Order, EngineError> {
        if ciphertext.len() > MAX_ENCRYPTED_PAYLOAD_BYTES {
            return Err(EngineError::Validation(
                DarkPoolError::EncryptedPayloadTooLarge {
                    max: MAX_ENCRYPTED_PAYLOAD_BYTES,
                },
            ));
        }

        // Cheap replay shed (#233): a byte-identical ciphertext is a replay of a
        // captured submission (ECIES is randomized), so reject it before the
        // ECIES decrypt + commitment derivation every admitted order pays. This
        // is advisory — the race-free, durable check is the same-lock test in
        // `persist_order_placed`; this only avoids spending crypto on a
        // ciphertext already known to be spent.
        if self
            .inner
            .state
            .lock()
            .seen_ciphertexts
            .contains(&ciphertext_digest(&ciphertext))
        {
            return Err(EngineError::Validation(DarkPoolError::DuplicateOrder));
        }

        let decrypter = self.inner.decrypter.read().clone();
        let decrypted = decrypter
            .decrypt(&ciphertext)
            .await
            .map_err(EngineError::Decrypt)?;

        if let Some(expected) = caller {
            if decrypted.trader != expected {
                return Err(EngineError::Validation(
                    DarkPoolError::TraderAddressMismatch {
                        expected: format!("{:#x}", expected),
                        found: "***".to_string(),
                    },
                ));
            }
        }

        // Plaintext validity is checked before proof verification so malformed
        // orders fail with precise validation errors. The proof, when a
        // verifier is installed, is checked after the engine derives the exact
        // commitment/nullifier public inputs from these fields.
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
        // price can be rejected. The same lock-scoped lookup resolves the asset
        // this leg spends for the solvency witness (#170/#213).
        let (spend_token, spend_decimals) = {
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
            // A bid (buyer) pays the quote token; an ask (seller) delivers the
            // base token. The solvency oracle reads `reserved` for exactly this
            // asset, so the witnessed balance matches the leg (#170).
            match decrypted.side {
                Side::Buy => (cfg.quote_token, cfg.quote_decimals),
                Side::Sell => (cfg.base_token, cfg.base_decimals),
            }
        };

        let default_ttl = self.inner.state.lock().default_ttl;
        let ttl = if decrypted.ttl > 0 {
            Duration::from_nanos(decrypted.ttl as u64).min(MAX_TTL)
        } else {
            default_ttl
        };

        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(600));

        let mut order = Order {
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
            // Placeholder; the real seq is the store-assigned OrderPlaced seq,
            // stamped onto both the book copy and this returned order below.
            seq: 0,
        };

        // Capture ZK witness secrets in-memory only. The salt is the
        // client-chosen blinding value carried inside the ciphertext (#217), so
        // the engine re-derives exactly the commitment the client proved
        // against — rather than minting a server-side salt the client cannot
        // know (#153). `trader_id` stays bound to the verified on-chain address.
        let salt = parse_order_salt(&decrypted.salt)?;
        // Read the trader's settlement-locked balance for the asset this leg
        // spends from the installed BalanceOracle (#213). Clone the Arc out so
        // the parking_lot read guard is not held across the await; a real
        // (chain-backed) oracle does an RPC here and fails the placement closed
        // if the read errors rather than fabricating a balance.
        let oracle = self.inner.oracle.read().clone();
        let (balance, position) = oracle
            .lookup(order.trader, spend_token, spend_decimals)
            .await
            .map_err(EngineError::BalanceLookup)?;
        let secrets = derive_order_secrets(
            order.trader.as_slice(),
            order.side as u8,
            order.price,
            order.size,
            balance,
            position,
            salt,
        )?;
        if let Some(verifier) = self.inner.order_proof_verifier.read().clone() {
            let publics = dp_zk::commitment_circuit::OrderProofPublics {
                commitment: dp_zk::pedersen::bytes32_to_scalar(&secrets.commitment)
                    .map_err(|_| EngineError::Validation(DarkPoolError::InvalidOrderProof))?,
                nullifier: dp_zk::pedersen::bytes32_to_scalar(&secrets.nullifier)
                    .map_err(|_| EngineError::Validation(DarkPoolError::InvalidOrderProof))?,
            };
            verifier
                .verify(&_unverified_proof, &publics)
                .map_err(|_| EngineError::Validation(DarkPoolError::InvalidOrderProof))?;
        }
        let commitment = secrets.commitment.to_vec();
        let nullifier = secrets.nullifier;

        // Stamp the returned order with the same seq the book copy got, so a
        // caller inspecting the result sees the real priority key, not 0.
        // `persist_order_placed` stores the witness secret atomically with the
        // book insert, so a racing tick can never observe a booked order without
        // its in-memory ZK witness.
        match self.persist_order_placed(
            order.clone(),
            secrets,
            commitment,
            _unverified_proof,
            ciphertext,
            nullifier,
        ) {
            Ok(seq) => order.seq = seq,
            Err(e) => return Err(e),
        }
        Ok(order)
    }

    fn ensure_order_capacity(
        state: &EngineState,
        trader: alloy_primitives::Address,
    ) -> Result<(), EngineError> {
        if state.book.active_order_count() >= MAX_ACTIVE_ORDERS {
            return Err(EngineError::Validation(
                DarkPoolError::OrderBookCapacityExceeded {
                    max: MAX_ACTIVE_ORDERS,
                },
            ));
        }
        if state.book.active_order_count_for_trader(trader.as_slice())
            >= MAX_ACTIVE_ORDERS_PER_TRADER
        {
            return Err(EngineError::Validation(
                DarkPoolError::TooManyRestingOrdersForTrader {
                    max: MAX_ACTIVE_ORDERS_PER_TRADER,
                },
            ));
        }
        Ok(())
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

    /// Commitment leaves for the orders admitted to a round — every order that
    /// was live in the book at tick time (#157). These form the round's
    /// admitted-set Merkle tree; membership of each settled leg is proven
    /// against its root, binding the operator to the publicly admitted input.
    ///
    /// Reads only the `secrets` lock, so it must never be called while holding
    /// the `state` lock: the established lock order is secrets→state (see
    /// [`Self::prune_dead_secrets`]), and inverting it risks deadlock. The tick
    /// captures the admitted order ids under the state lock and resolves their
    /// commitments here, on the lock-free async proof path. An order whose
    /// secret has already been pruned is skipped; a *matched* leg that ends up
    /// absent is caught downstream by `from_witness_with_admitted`.
    pub(crate) fn admitted_commitments(&self, order_ids: &[Uuid]) -> Vec<ark_bn254::Fr> {
        let secrets = self.inner.secrets.lock();
        order_ids
            .iter()
            .filter_map(|id| secrets.get(id))
            .map(|s| {
                dp_zk::pedersen::bytes32_to_scalar(&s.commitment)
                    .expect("engine-derived commitments are canonical field encodings")
            })
            .collect()
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

    pub(crate) fn persist_order_placed(
        &self,
        mut order: Order,
        secrets: OrderSecrets,
        commitment: Vec<u8>,
        proof: Vec<u8>,
        ciphertext: Vec<u8>,
        nullifier: [u8; 32],
    ) -> Result<u64, EngineError> {
        // Hash the ciphertext now, before it is moved into the event below;
        // this is the replay/spent-set key (#233).
        let digest = ciphertext_digest(&ciphertext);
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

        let mut secrets_guard = self.inner.secrets.lock();
        let mut state = self.inner.state.lock();
        // Re-validate the pair status under the *same* lock that does the
        // append + insert. The status check in `place_encrypted_order` runs
        // in an earlier, separately-acquired lock scope; between dropping
        // that guard and taking this one, `suspend_pair` / `delist_pair` can
        // flip the pair off `Active`. Re-checking here closes the TOCTOU
        // (issue #162): a delist snapshots open-order ids and cancels them,
        // and an order inserted after that snapshot would otherwise land in a
        // delisted pair's book — never matched, never cancelled, escrow
        // stranded. The check must be atomic with the insert, so it lives
        // inside this guard rather than re-checking and then locking again.
        match state.pair_config(&order.pair).map(|c| c.status) {
            Some(crate::state::PairStatus::Active) => {}
            Some(_) => {
                return Err(EngineError::Validation(DarkPoolError::PairNotAccepting(
                    order.pair.clone(),
                )));
            }
            None => {
                return Err(EngineError::Validation(DarkPoolError::PairNotRegistered(
                    order.pair.clone(),
                )));
            }
        }
        // Reject a replayed ciphertext (#233), under the same lock that appends
        // and inserts so two concurrent identical submissions can't both pass.
        if state.seen_ciphertexts.contains(&digest) {
            return Err(EngineError::Validation(DarkPoolError::DuplicateOrder));
        }
        if state.spent_nullifiers.contains(&nullifier) {
            return Err(EngineError::Validation(DarkPoolError::DuplicateOrder));
        }
        Self::ensure_order_capacity(&state, order.trader)?;
        self.inner.store.append(&mut events)?;
        let evt = &events[0];
        // The store stamped the monotonic seq onto the event during append;
        // adopt it as the order's price-time priority key so the live book and
        // a replayed book agree on matching order (issue #161, ADR 0005).
        order.seq = evt.seq;
        let assigned_seq = order.seq;
        state.book.apply(evt);
        let side_label = match order.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };
        let order_id = order.id;
        state.book.insert_order(order);
        secrets_guard.insert(order_id, secrets);
        // Record the digest only after the event is durably appended, so the
        // live spent-set and a replay rebuilt from the log stay in lockstep.
        state.seen_ciphertexts.insert(digest);
        state.spent_nullifiers.insert(nullifier);
        metrics::counter!(M_ORDERS_PLACED, "side" => side_label).increment(1);
        Ok(assigned_seq)
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

    /// Resting orders owned by `trader`, across every pair, oldest first.
    /// Backs the caller-scoped "my orders" API surface: a trader may only
    /// ever see their own orders, never the cross-trader book.
    pub fn orders_by_trader(&self, trader: alloy_primitives::Address) -> Vec<Order> {
        let state = self.inner.state.lock();
        let mut out: Vec<Order> = state
            .book
            .iter_all()
            .into_iter()
            .filter(|o| o.trader == trader)
            .collect();
        out.sort_by(|a, b| a.submitted_at.cmp(&b.submitted_at));
        out
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
        // Circuit checks this in the asset the leg spends: quote for a bid,
        // base for an ask (#170). `place_encrypted_order` already read the
        // BalanceOracle for that exact asset (#213), so `secret.balance` is
        // denominated to match the leg.
        balance: secret.balance,
        position: secret.position.to_string(),
        limit_price: order.price,
        order_size: order.size,
        side,
        // The circuit identity is the on-chain settlement address; `trader_id`
        // above is `poseidon(this)` (#153).
        trader_addr: hex::encode(order.trader),
    }
}

/// Derive per-order ZK secrets.
///
/// **Salt source (#217).** The `salt` is the client-chosen blinding value
/// carried inside the ciphertext, decoded by [`parse_order_salt`]. The client
/// commits and proves the order against it, so the engine re-derives the
/// identical commitment here — that is what makes the per-order proof
/// verifiable at ingestion. `trader_id` stays bound to the verified on-chain
/// address (#153), not the client-chosen `commitment_key`, so a client cannot
/// forge another trader's identity by choosing the salt.
// Internal helper threading the full secret-derivation inputs; the argument
// list is intentionally flat rather than a one-off params struct.
#[allow(clippy::too_many_arguments)]
fn derive_order_secrets(
    trader_addr: &[u8],
    side: u8,
    price: Decimal,
    size: Decimal,
    balance: Decimal,
    position: i128,
    salt: [u8; 32],
) -> Result<OrderSecrets, EngineError> {
    let trader_id = dp_zk::pedersen::derive_trader_id_bytes(trader_addr)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    let commitment_fr = compute_poseidon_commitment_fr(&trader_id, side, price, size, &salt)?;
    let nullifier_fr = dp_zk::commitment_circuit::compute_nullifier_native(
        commitment_fr,
        dp_zk::pedersen::bytes32_to_scalar(&salt)
            .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?,
    );

    // `balance`/`position` are read from the installed BalanceOracle by the
    // caller (#213) — in the asset this leg spends (#170) — and threaded in
    // here so this helper stays pure and synchronous.
    Ok(OrderSecrets {
        salt,
        trader_id,
        commitment: dp_zk::fr_to_bytes32(commitment_fr),
        nullifier: dp_zk::fr_to_bytes32(nullifier_fr),
        balance,
        position,
    })
}

/// Recompute the persisted Poseidon commitment for a (trader_addr, side, price,
/// size, salt) tuple. Used by recovery to re-verify `OrderPlaced` events
/// without keeping any state in memory. The `salt` is the client's, read back
/// from the decrypted ciphertext (#217).
pub(crate) fn recompute_persisted_commitment(
    trader_addr: &[u8],
    side: u8,
    price: Decimal,
    size: Decimal,
    salt: &[u8; 32],
) -> Result<[u8; 32], EngineError> {
    let trader_id = dp_zk::pedersen::derive_trader_id_bytes(trader_addr)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    compute_poseidon_commitment(&trader_id, side, price, size, salt)
}

/// SHA-256 of an order's ciphertext — the key for the replay spent-set (#233).
/// ECIES is randomized, so a byte-identical ciphertext is a replay of a captured
/// submission, not a fresh order. The live `persist_order_placed` path and the
/// recovery replay path MUST hash identically, so both call this one helper.
pub(crate) fn ciphertext_digest(ciphertext: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(ciphertext);
    h.finalize().into()
}

/// Decode the client's `salt` (lowercase hex inside the ciphertext, #217) into
/// the 32 raw bytes the commitment and nullifier are derived from. A malformed
/// salt is a malformed order, surfaced to the client as `SaltInvalid` rather
/// than an internal fault.
pub(crate) fn parse_order_salt(salt_hex: &str) -> Result<[u8; 32], EngineError> {
    if salt_hex.len() != 64
        || !salt_hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(EngineError::Validation(DarkPoolError::SaltInvalid));
    }
    let bytes: [u8; 32] = hex::decode(salt_hex)
        .map_err(|_| EngineError::Validation(DarkPoolError::SaltInvalid))?
        .try_into()
        .map_err(|_| EngineError::Validation(DarkPoolError::SaltInvalid))?;
    dp_zk::pedersen::bytes32_to_scalar(&bytes)
        .map_err(|_| EngineError::Validation(DarkPoolError::SaltInvalid))?;
    Ok(bytes)
}

/// Compute the Poseidon order commitment (matches the in-circuit gadget
/// byte-for-byte). Inputs use the canonical ZK encodings:
/// - `trader_id` and `salt` are 32 BE bytes mapped into Fr via modular
///   canonical field decoding.
/// - `price` / `size` go through `decimal_to_scalar` (1e8 fixed-point).
/// - `side` 0 → Fr::zero, otherwise Fr::one.
pub(crate) fn compute_poseidon_commitment(
    trader_id: &[u8; 32],
    side: u8,
    price: Decimal,
    size: Decimal,
    salt: &[u8; 32],
) -> Result<[u8; 32], EngineError> {
    Ok(dp_zk::fr_to_bytes32(compute_poseidon_commitment_fr(
        trader_id, side, price, size, salt,
    )?))
}

fn compute_poseidon_commitment_fr(
    trader_id: &[u8; 32],
    side: u8,
    price: Decimal,
    size: Decimal,
    salt: &[u8; 32],
) -> Result<ark_bn254::Fr, EngineError> {
    let trader_fr = dp_zk::pedersen::bytes32_to_scalar(trader_id)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    let salt_fr = dp_zk::pedersen::bytes32_to_scalar(salt)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    let input = dp_zk::OrderCommitmentInput::from_decimals(trader_fr, side, price, size, salt_fr)
        .map_err(|e| EngineError::CommitmentEncoding(e.to_string()))?;
    Ok(dp_zk::commit_native(&input))
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
    // Random per call so two otherwise-identical orders still get distinct
    // commitments (the live client picks a fresh salt per order, #217).
    let mut salt = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let d = dp_crypto::DecryptedOrder {
        trader,
        pair: pair.to_string(),
        side,
        price,
        size,
        commitment_key: commitment_key.to_string(),
        salt: hex::encode(salt),
        ttl: ttl.as_nanos() as i64,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    // Engine no longer verifies the client-supplied commitment, so a
    // placeholder is sufficient. The real commitment is recomputed inside
    // the engine via `compute_poseidon_commitment`.
    (vec![0u8; 32], ct)
}
