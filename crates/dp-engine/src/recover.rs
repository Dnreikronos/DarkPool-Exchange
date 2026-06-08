use std::collections::HashMap;
use std::time::Duration;

use alloy_primitives::U256;
use chrono::{DateTime, Utc};
use dp_crypto::{Decrypter, SnapshotCipher};
use dp_event::{Event, EventData, EventError, SnapshotStore, Store};
use dp_types::{EventType, Order, Pair};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::engine::Engine;
use crate::error::EngineError;
use crate::snapshot::{decode_envelope, SnapshotError};
use crate::state::{try_build_settlement_row, AuctionExecutedRecord, PairConfig, PendingBatch};

/// Result of the snapshot recovery probe. Drives the recover() control
/// flow without leaking the multi-arm match through the caller.
enum SnapshotRecoveryOutcome {
    /// One of the envelopes decoded cleanly; `after_seq` is set to its
    /// covered sequence.
    Restored(u64),
    /// `list_seqs` is empty — there is no snapshot to try.
    NoneAvailable,
    /// Every envelope on the store failed to decode. Caller must check
    /// whether the event log is still complete before falling back to a
    /// full replay.
    AllCorrupt,
    /// The store itself errored (I/O, db). Caller logs and falls back
    /// to a full replay, mirroring pre-hardening behaviour.
    StoreError(EventError),
}

/// Apply a decoded snapshot and populate `placed_orders` from the restored
/// book. Logs whether this was a first-attempt or fallback recovery, and
/// warns when the envelope's self-reported seq disagrees with the store key
/// (the envelope value is authoritative: the seq is bound into the AEAD
/// associated data, so a tampered seq fails the tag rather than being trusted).
fn apply_and_log_snapshot(
    engine: &Engine,
    placed_orders: &mut HashMap<Uuid, Order>,
    snap_state: crate::state::SerializableState,
    decoded_seq: u64,
    store_key: u64,
    attempts: usize,
) {
    engine.apply_snapshot_state(snap_state);
    for o in engine.inner.state.lock().book.iter_all() {
        placed_orders.insert(o.id, o);
    }
    if attempts == 1 {
        tracing::info!(seq = decoded_seq, "recovered from snapshot");
    } else {
        tracing::warn!(
            seq = decoded_seq,
            attempts,
            "recovered from older snapshot — newer envelopes were corrupt"
        );
    }
    if decoded_seq != store_key {
        tracing::warn!(
            envelope_seq = decoded_seq,
            store_key,
            "snapshot envelope seq disagrees with store key — \
             trusting envelope (seq is AEAD-tag-authenticated)"
        );
    }
}

/// Probe the snapshot store for the most-recent decodable envelope.
/// Walks `list_seqs` descending so a single corrupt latest does not
/// strand recovery when older envelopes are retained.
fn try_restore_from_snapshots(
    engine: &Engine,
    snap_store: &dyn SnapshotStore,
    cipher: Option<&SnapshotCipher>,
    placed_orders: &mut HashMap<Uuid, Order>,
) -> SnapshotRecoveryOutcome {
    let seqs = match snap_store.list_seqs() {
        Ok(s) => s,
        Err(e) => return SnapshotRecoveryOutcome::StoreError(e),
    };
    if seqs.is_empty() {
        return SnapshotRecoveryOutcome::NoneAvailable;
    }
    // Envelopes are AEAD-sealed (#203); without a cipher they cannot be read.
    // Treat that as "all corrupt" so the caller applies the same safety net it
    // uses for genuinely unreadable envelopes (full replay only when the event
    // log still covers seq 1, else refuse to boot).
    let Some(cipher) = cipher else {
        tracing::error!(
            "snapshot envelopes present but no SnapshotCipher configured — cannot \
             decrypt; set DARKPOOL_SNAPSHOT_KEY_URI. Falling back to event replay."
        );
        return SnapshotRecoveryOutcome::AllCorrupt;
    };
    let mut attempts = 0usize;
    let mut decrypt_failures = 0usize;
    for &seq in seqs.iter().rev() {
        let bytes = match snap_store.read_at(seq) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(error = ?e, seq, "snapshot read_at failed; trying older envelope");
                continue;
            }
        };
        attempts += 1;
        match decode_envelope(&bytes, cipher) {
            Ok((decoded_seq, snap_state)) => {
                apply_and_log_snapshot(
                    engine,
                    placed_orders,
                    snap_state,
                    decoded_seq,
                    seq,
                    attempts,
                );
                return SnapshotRecoveryOutcome::Restored(decoded_seq);
            }
            Err(e) => {
                // An AEAD-open failure is indistinguishable from on-disk
                // corruption at this layer — that's inherent to AEAD. But if
                // *every* envelope fails this exact way, the likeliest cause is
                // the wrong snapshot key, not real corruption; count it so the
                // post-loop summary can surface that hint.
                if matches!(e, SnapshotError::Decrypt) {
                    decrypt_failures += 1;
                }
                tracing::warn!(error = ?e, seq, "snapshot envelope corrupt; trying older");
                // `decode_envelope` is pure and runs before
                // `apply_snapshot_state`, so the engine's state was never
                // touched on this iteration. No reset needed — `placed_orders`
                // is also untouched on this branch for the same reason.
            }
        }
    }
    // Every envelope failed to decode. When all failures were AEAD-open
    // failures — no BadMagic / Truncated / version / length mismatch mixed in —
    // the envelopes are well-formed but unreadable under this key, the classic
    // signature of a wrong or freshly-rotated snapshot key. Surface a targeted
    // hint so the operator checks the key before assuming the snapshots are
    // lost; the caller still applies its usual safety net regardless.
    if attempts > 0 && decrypt_failures == attempts {
        tracing::error!(
            envelopes = attempts,
            "every snapshot envelope failed AEAD authentication — as consistent \
             with a WRONG snapshot key (check DARKPOOL_SNAPSHOT_KEY_URI or a recent \
             key rotation) as with on-disk corruption"
        );
    }
    SnapshotRecoveryOutcome::AllCorrupt
}

/// True iff the event log can produce a complete from-scratch replay —
/// i.e. its first event has seq 1. False if the log was compacted
/// (first seq > 1) OR is entirely empty while snapshots claim history
/// existed. Both cases are unsafe to silently boot from when every
/// snapshot envelope is corrupt: the empty-log case would yield an
/// empty engine that contradicts the (now-unreadable) snapshot
/// history, and the compacted case would rebuild a partial book.
///
/// Couples to [`dp_event::assign_seq_and_timestamp`], which increments
/// from 0 → 1 on the first write. If that ever changes, this check
/// must move in lockstep — searching for `e.seq == 1` finds both
/// sites.
fn event_log_covers_seq_one(store: &dyn Store) -> Result<bool, EngineError> {
    let first = store.read_from(0, 1)?;
    Ok(first.first().is_some_and(|e| e.seq == 1))
}

/// True iff the event log has no gap immediately after `after_seq`.
/// After a successful snapshot restore, tail events must form a
/// continuous run: the first readable event must be `after_seq + 1`
/// or the log must be empty past that point (nothing to replay).
/// A gap means compaction removed events that are not covered by the
/// snapshot, which would produce a partial projection on boot.
fn event_log_continuous_after(store: &dyn Store, after_seq: u64) -> Result<bool, EngineError> {
    let next = store.read_from(after_seq, 1)?;
    Ok(match next.first() {
        Some(e) => e.seq == after_seq + 1,
        None => true,
    })
}

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
        let snapshot_cipher = self.inner.snapshot_cipher.read().clone();
        let aggregator = self.inner.aggregator.read().clone();
        let store = self.inner.store.clone();

        let (default_ttl, _) = {
            let mut state = self.inner.state.lock();
            state.reset_projection();
            (state.default_ttl, ())
        };

        // First pass: replay events, decrypting + applying without holding lock
        // across .await. We hold the lock briefly per-event to mutate state.
        //
        // `placed_orders` snapshots every `OrderPlaced` we replay so the
        // settlement-match builder below can still resolve trader / pair
        // metadata for orders that `OrderMatched` (full-fill) has since
        // removed from `state.book`. The live tick path captures an
        // equivalent snapshot *before* fill events apply (see `tick.rs`);
        // recovery must do the same or full-fill matches lose their
        // settlement rows on restart.
        let mut after_seq: u64 = 0;
        let mut matches_by_auction: HashMap<Uuid, Vec<OrphanMatch>> = HashMap::new();
        let mut auction_timestamps: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
        let mut placed_orders: HashMap<Uuid, Order> = HashMap::new();

        // Snapshot fast-path. When a snapshot store is installed and an
        // envelope decodes cleanly, restore the projection-derived state
        // and skip the event-replay loop ahead to its sequence.
        //
        // Corruption is handled in layers:
        //   1. `read_latest` decode failure → walk `list_seqs` descending
        //      and try each older envelope. The periodic snapshotter
        //      keeps `retain_count` (default 3) envelopes precisely so a
        //      bad latest does not strand recovery — without this walk
        //      the older envelopes were dead weight.
        //   2. If every snapshot fails AND the event log cannot
        //      reproduce history from seq 1 (compacted past it, or
        //      entirely empty while snapshots existed), refuse to boot.
        //      Either case would silently boot state that contradicts
        //      the (now-unreadable) snapshot history — exactly the
        //      "wrong state on restart" failure mode the snapshotter
        //      is supposed to prevent.
        //   3. Only when the event log still starts at seq 1 do we fall
        //      back to a full replay.
        if let Some(snap_store) = self.inner.snapshot_store.read().clone() {
            let restored = try_restore_from_snapshots(
                self,
                snap_store.as_ref(),
                snapshot_cipher.as_deref(),
                &mut placed_orders,
            );
            match restored {
                SnapshotRecoveryOutcome::Restored(seq) => {
                    if !event_log_continuous_after(store.as_ref(), seq)? {
                        return Err(EngineError::SnapshotsCorruptAndLogTruncated);
                    }
                    after_seq = seq;
                }
                SnapshotRecoveryOutcome::NoneAvailable => {
                    tracing::info!("no snapshot present, full replay");
                }
                SnapshotRecoveryOutcome::AllCorrupt => {
                    // Every envelope on the store failed to decode.
                    // Full replay is only safe when the event log can
                    // genuinely reproduce history from seq 1 onward —
                    // otherwise we'd silently boot wrong state.
                    if !event_log_covers_seq_one(store.as_ref())? {
                        return Err(EngineError::SnapshotsCorruptAndLogTruncated);
                    }
                    tracing::warn!(
                        "every snapshot envelope corrupt; event log intact from seq 1 — full replay"
                    );
                    self.inner.state.lock().reset_projection();
                }
                SnapshotRecoveryOutcome::StoreError(e) => {
                    tracing::warn!(error = ?e, "snapshot store read failed; falling back to full replay");
                    if !event_log_covers_seq_one(store.as_ref())? {
                        return Err(EngineError::SnapshotsCorruptAndLogTruncated);
                    }
                    self.inner.state.lock().reset_projection();
                }
            }
        }

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
                    &mut placed_orders,
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
            let match_count = matches.len();
            let proof = match aggregator
                .aggregate(batch_id, auction_id, &matches, &witness)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    log_orphan_reaggregate_error(auction_id, match_count, &e);
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

            let settlement_matches =
                build_recovery_settlement_matches(&matches, &placed_orders, &state.pair_tokens);
            // IVC path: public inputs are not used for on-chain verification;
            // store zeros to satisfy the PendingBatch schema.
            let public_inputs = [U256::ZERO; 6];

            state.pending_batches.insert(
                batch_id,
                PendingBatch {
                    batch_id,
                    auction_id,
                    matches: matches.clone(),
                    settlement_matches,
                    proof,
                    public_inputs,
                    // IVC batches are intentionally non-retryable after
                    // restart: the in-memory folding accumulator is lost, so
                    // the stored proof_bytes cannot be paired with a fresh
                    // SubmitSessionParams (z_0/z_n/n_steps). `submit_batch`
                    // fires the poison-gate error, blocking retry until an
                    // operator intervenes.
                    ivc_payload: None,
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
        placed_orders: &mut HashMap<Uuid, Order>,
    ) -> Result<(), EngineError> {
        match &ev.data {
            EventData::OrderPlaced {
                order_id,
                commitment,
                ciphertext,
                salt_nonce,
                ..
            } => {
                let decrypted = decrypter.decrypt(ciphertext).await.map_err(|source| {
                    EngineError::RecoverDecrypt {
                        order_id: *order_id,
                        source,
                    }
                })?;
                // A persisted salt_nonce of != 32 bytes cannot be the one the
                // commitment was originally derived from. Surface the bad
                // length as a distinct error instead of silently falling back
                // to a zero nonce — the zero fallback would have produced a
                // misleading `RecoverCommitmentMismatch` for every affected
                // event, hiding the actual corruption.
                let nonce: [u8; 32] = salt_nonce.as_slice().try_into().map_err(|_| {
                    EngineError::RecoverSaltNonceLen {
                        order_id: *order_id,
                        len: salt_nonce.len(),
                    }
                })?;
                let recomputed = crate::engine::recompute_persisted_commitment(
                    *order_id,
                    decrypted.trader.as_slice(),
                    &decrypted.commitment_key,
                    decrypted.side as u8,
                    decrypted.price,
                    decrypted.size,
                    &nonce,
                )?;
                if recomputed.as_slice() != commitment.as_slice() {
                    return Err(EngineError::RecoverCommitmentMismatch {
                        order_id: *order_id,
                    });
                }
                let ttl = if decrypted.ttl > 0 {
                    Duration::from_nanos(decrypted.ttl as u64).min(crate::engine::MAX_TTL)
                } else {
                    default_ttl
                };
                let expires_at = ev.timestamp
                    + chrono::Duration::from_std(ttl)
                        .unwrap_or_else(|_| chrono::Duration::seconds(600));
                // Mirror the live path's canonicalisation
                // (`Engine::place_encrypted_order`). Otherwise an event with
                // pre-canonicalisation or trader-cased `pair` ("eth/usdc")
                // would land in the book under that key while the registry
                // iterates canonical keys ("ETH/USDC") — the auction loop
                // would never match the order and `get_order_book` for
                // either casing would miss it.
                let pair_key = Pair::parse(&decrypted.pair)?.into_string();
                let order = Order {
                    id: *order_id,
                    trader: decrypted.trader,
                    pair: pair_key.clone(),
                    side: decrypted.side,
                    price: decrypted.price,
                    size: decrypted.size,
                    remaining_size: decrypted.size,
                    commitment_key: decrypted.commitment_key.clone(),
                    encrypted_payload: ciphertext.clone(),
                    submitted_at: ev.timestamp,
                    expires_at,
                    // Priority key is the event's own seq — the same value the
                    // live path stamped at placement (issue #161, ADR 0005), so
                    // replay reconstructs the identical matching order even
                    // though `submitted_at` here is the (DB-write) event time.
                    seq: ev.seq,
                };
                placed_orders.insert(order.id, order.clone());
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                // Legacy compatibility: event logs predating the pair
                // registry have no PairRegistered entries. Lazily seed
                // a default-status registry entry so the auction loop and
                // settlement path can still find pair metadata. New event
                // logs always emit `PairRegistered` first.
                state.pair_tokens.entry(pair_key).or_default();
                state.book.insert_order(order);
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

                // Snapshot-aware insert: if recovery loaded a snapshot
                // that already carried this batch, do not overwrite it
                // with a freshly-rebuilt one. The snapshot's record has
                // the original `matches` / `settlement_matches` /
                // `public_inputs`; rebuilding from a post-snapshot
                // replay can only produce empty matches (the
                // corresponding OrderMatched events were
                // pre-snapshot), which would defeat the purpose of
                // keeping the snapshot in the first place.
                if state.pending_batches.contains_key(batch_id) {
                    return Ok(());
                }

                let settlement_matches =
                    build_recovery_settlement_matches(&matches, placed_orders, &state.pair_tokens);
                // FUTURE: persist `public_inputs` in the `BatchSubmitted`
                // event so a crash-then-restart can resubmit unconfirmed
                // batches. Today we store zeros because the witness
                // secrets that drive the live computation are wiped on
                // restart; `submit_batch` guards against actually sending
                // these zeros (poisoned-batch path) so the operator gets
                // a loud error instead of a failed on-chain verification.
                let public_inputs = [U256::ZERO; 6];

                state.pending_batches.insert(
                    *batch_id,
                    PendingBatch {
                        batch_id: *batch_id,
                        auction_id: *auction_id,
                        matches,
                        settlement_matches,
                        proof: proof.clone(),
                        public_inputs,
                        // IVC batches are intentionally non-retryable after
                        // restart: the in-memory folding accumulator is lost,
                        // so the stored proof_bytes are the only artifact —
                        // but the matching SubmitSessionParams (z_0/z_n/
                        // n_steps) cannot be reconstructed without it.
                        // `submit_batch` will fire the poison-gate error,
                        // blocking retry until an operator intervenes.
                        ivc_payload: None,
                        attempts: 0,
                        next_attempt: None,
                        submitting: false,
                    },
                );
            }
            EventData::BatchConfirmed { batch_id, .. }
            | EventData::BatchSettled { batch_id, .. } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                state.pending_batches.remove(batch_id);
            }
            EventData::PairRegistered {
                pair,
                base_token,
                quote_token,
                min_order_size,
                tick_size,
                auction_interval_ms,
            } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                let base = parse_address_or_zero_logged(base_token, pair, "base_token");
                let quote = parse_address_or_zero_logged(quote_token, pair, "quote_token");
                // Defensive canonicalisation: the live writer
                // (`Engine::register_pair_with_event`) always canonicalises
                // before persisting, so today this is a no-op. The
                // OrderPlaced replay path canonicalises decrypted.pair for
                // the same reason; mirror the stance here in case a legacy
                // / hand-crafted event log holds a non-canonical pair
                // string (otherwise the registry would diverge from the
                // book key both pipelines use).
                let key = canonicalise_replayed_pair(pair);
                state.pair_tokens.insert(
                    key,
                    crate::state::PairConfig {
                        base_token: base,
                        quote_token: quote,
                        min_order_size: *min_order_size,
                        tick_size: *tick_size,
                        auction_interval: auction_interval_ms.map(Duration::from_millis),
                        status: crate::state::PairStatus::Active,
                    },
                );
            }
            EventData::PairSuspended { pair } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                let key = canonicalise_replayed_pair(pair);
                if let Some(cfg) = state.pair_tokens.get_mut(&key) {
                    cfg.status = crate::state::PairStatus::Suspended;
                }
            }
            EventData::PairDelisted { pair } => {
                let mut state = self.inner.state.lock();
                state.book.apply(ev);
                let key = canonicalise_replayed_pair(pair);
                if let Some(cfg) = state.pair_tokens.get_mut(&key) {
                    cfg.status = crate::state::PairStatus::Delisted;
                }
            }
            EventData::BatchFolded {
                round_index, pair, ..
            } => {
                // Rebuild the per-pair round counter so the finalization
                // boundary fires at the right offset after a restart.
                let mut map = self.inner.ivc_round.lock();
                let entry = map.entry(pair.clone()).or_insert(0);
                *entry = (*entry).max(*round_index);
            }
        }
        Ok(())
    }
}

/// Canonicalise a `pair` string read from a replayed pair-lifecycle
/// event. The live writers always canonicalise before persist, so the
/// canonicalised form usually equals the input — this is defensive
/// against legacy logs or hand-crafted events with non-canonical
/// strings. Unparseable strings fall back to the original so a single
/// bad event doesn't brick the boot path; the matching book entry
/// (also fed through the original string) would be the same broken
/// key, so the failure mode is at least internally consistent.
fn canonicalise_replayed_pair(pair: &str) -> String {
    Pair::parse(pair)
        .map(|p| p.into_string())
        .unwrap_or_else(|_| pair.to_string())
}

/// Build settlement rows from `matches` using an order-id snapshot.
/// Trader / pair are resolved from `placed_orders` (captured before
/// fills applied) and pair-token addresses from `pair_tokens`. Matches
/// whose order or pair config is missing are silently skipped —
/// recovery may see orphan-match events whose `OrderPlaced` events fell
/// off the log, or whose pair was never registered.
fn build_recovery_settlement_matches(
    matches: &[dp_auction::Match],
    placed_orders: &HashMap<Uuid, Order>,
    pair_tokens: &HashMap<String, PairConfig>,
) -> Vec<dp_settlement::SettlementMatch> {
    matches
        .iter()
        .filter_map(|m| try_build_settlement_row(m, placed_orders, pair_tokens).ok())
        .collect()
}

/// Parse a token address from a `PairRegistered` event, falling back to
/// `Address::ZERO` on parse failure. A malformed address would silently
/// become zero previously — settlement would then build matches with a
/// zero recipient, sending funds nowhere. We log a warning on the
/// fallback so the corruption surfaces at recovery time without
/// bricking the boot path on a single bad legacy event.
fn parse_address_or_zero_logged(hex: &str, pair: &str, field: &str) -> alloy_primitives::Address {
    match hex.parse::<alloy_primitives::Address>() {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(
                pair = pair,
                field = field,
                raw = hex,
                "malformed token address in PairRegistered event; falling back to 0x0 — \
                 settlement for this pair will route to the zero address until the \
                 registry is corrected: {e}"
            );
            alloy_primitives::Address::ZERO
        }
    }
}

/// Pre-format then log. Pulled out of `recover` so the error branch is a
/// single linear sequence of statements rather than a fold of macro
/// expansions: under llvm-cov the lazy `tracing::error!(field = expr)`
/// form leaves the captured expressions as uncovered regions even when
/// the surrounding branch runs.
fn log_orphan_reaggregate_error(
    auction_id: Uuid,
    match_count: usize,
    err: &dp_aggregator::AggregatorError,
) {
    let msg = format!(
        "orphan re-aggregate failed for auction {auction_id} ({match_count} matches); settlement event will NOT be replayed: {err}"
    );
    tracing::error!("{msg}");
}

/// Retained as a test-only helper. In Phase G the IVC recovery path stores
/// zeros directly; this function is exercised by unit tests to cover the
/// error-log formatting.
#[cfg(test)]
fn log_compute_public_inputs_warn(batch_id: Uuid, err: &dp_zk::ZkError) {
    let msg = format!(
        "compute_public_inputs failed for batch {batch_id} during recovery: {err}; using zeros"
    );
    tracing::warn!("{msg}");
}

/// Retained as a test-only helper — see [`log_compute_public_inputs_warn`].
#[cfg(test)]
fn finalize_public_inputs<T>(
    result: Result<T, dp_zk::ZkError>,
    batch_id: Uuid,
    convert: impl FnOnce(T) -> [U256; 6],
) -> [U256; 6] {
    match result {
        Ok(t) => convert(t),
        Err(e) => {
            log_compute_public_inputs_warn(batch_id, &e);
            [U256::ZERO; 6]
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Address;
    use chrono::Utc;
    use dp_event::{EventData, MemStore};
    use dp_types::{EventType, Fill, Order, Side};
    use rust_decimal::Decimal;

    use super::*;
    use crate::state::{EngineState, PairConfig};

    fn order_at(id: Uuid, pair: &str, side: Side, trader: Address) -> Order {
        Order {
            id,
            trader,
            pair: pair.to_string(),
            side,
            price: Decimal::ONE,
            size: Decimal::TEN,
            remaining_size: Decimal::ONE,
            commitment_key: "ck".into(),
            encrypted_payload: Vec::new(),
            submitted_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(600),
            seq: 0,
        }
    }

    #[test]
    fn finalize_public_inputs_err_returns_zeros_and_logs() {
        let err: Result<[U256; 6], dp_zk::ZkError> = Err(dp_zk::ZkError::Witness("demo".into()));
        let out = super::finalize_public_inputs(err, Uuid::nil(), |x| x);
        assert_eq!(out, [U256::ZERO; 6]);
    }

    #[test]
    fn finalize_public_inputs_ok_applies_convert() {
        let ok: Result<u64, dp_zk::ZkError> = Ok(7);
        let out = super::finalize_public_inputs(ok, Uuid::nil(), |n| {
            let mut a = [U256::ZERO; 6];
            a[0] = U256::from(n);
            a
        });
        assert_eq!(out[0], U256::from(7u64));
    }

    #[test]
    fn log_helpers_are_callable() {
        // Direct unit-test of the log helpers so the format! + tracing
        // calls in each are covered. Their wrappers in recover() are only
        // reachable on error paths that the integration tests cover when
        // possible; compute_public_inputs in particular doesn't fail in
        // practice on the empty-witness path the orphan branch uses, so
        // this is the only way to exercise the warn helper.
        let agg_err = dp_aggregator::AggregatorError::ProcessFailed {
            exit_code: 7,
            stderr: "demo".into(),
        };
        super::log_orphan_reaggregate_error(Uuid::nil(), 3, &agg_err);

        let zk_err = dp_zk::ZkError::Witness("demo".into());
        super::log_compute_public_inputs_warn(Uuid::nil(), &zk_err);
    }

    #[test]
    fn parse_address_or_zero_logged_parses_valid() {
        let addr: Address = "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap();
        assert_eq!(
            parse_address_or_zero_logged(
                "0x0000000000000000000000000000000000000001",
                "ETH/USDC",
                "base_token",
            ),
            addr
        );
    }

    #[test]
    fn parse_address_or_zero_logged_falls_back_to_zero_on_invalid() {
        assert_eq!(
            parse_address_or_zero_logged("not-an-address", "ETH/USDC", "base_token"),
            Address::ZERO
        );
    }

    #[test]
    fn build_recovery_settlement_matches_skips_missing_orders() {
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let matches = vec![dp_auction::Match {
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
        }];
        let out = build_recovery_settlement_matches(&matches, &HashMap::new(), &HashMap::new());
        assert!(out.is_empty());
    }

    #[test]
    fn build_recovery_settlement_matches_skips_when_pair_not_in_registry() {
        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut placed = HashMap::new();
        placed.insert(
            bid_id,
            order_at(bid_id, "UNKNOWN/PAIR", Side::Buy, Address::repeat_byte(1)),
        );
        placed.insert(
            ask_id,
            order_at(ask_id, "UNKNOWN/PAIR", Side::Sell, Address::repeat_byte(2)),
        );
        let matches = vec![dp_auction::Match {
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
        }];
        let out = build_recovery_settlement_matches(&matches, &placed, &HashMap::new());
        assert!(out.is_empty());
    }

    #[test]
    fn build_recovery_settlement_matches_builds_row_when_pair_registered() {
        let base = Address::repeat_byte(0xAA);
        let quote = Address::repeat_byte(0xBB);
        let mut pair_tokens = HashMap::new();
        pair_tokens.insert("ETH/USDC".to_string(), PairConfig::new(base, quote));

        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let bid_trader = Address::repeat_byte(1);
        let ask_trader = Address::repeat_byte(2);
        let mut placed = HashMap::new();
        placed.insert(bid_id, order_at(bid_id, "ETH/USDC", Side::Buy, bid_trader));
        placed.insert(ask_id, order_at(ask_id, "ETH/USDC", Side::Sell, ask_trader));

        let matches = vec![dp_auction::Match {
            bid: Fill {
                order_id: bid_id,
                size: Decimal::ONE,
            },
            ask: Fill {
                order_id: ask_id,
                size: Decimal::ONE,
            },
            price: Decimal::new(2000, 0),
            size: Decimal::ONE,
        }];
        let out = build_recovery_settlement_matches(&matches, &placed, &pair_tokens);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bid_order_id, bid_id);
        assert_eq!(out[0].ask_order_id, ask_id);
        assert_eq!(out[0].bid_trader, bid_trader);
        assert_eq!(out[0].ask_trader, ask_trader);
        assert_eq!(out[0].base_token, base);
        assert_eq!(out[0].quote_token, quote);
        assert_eq!(out[0].price, Decimal::new(2000, 0));
        assert_eq!(out[0].size, Decimal::ONE);
    }

    /// Regression for the fully-filled-order recovery bug:
    /// `OrderMatched` apply removed the orders from the book before
    /// `recover_settlement_matches` looked them up — so full-fill matches
    /// produced empty settlement rows. Snapshot-based lookup keeps the
    /// trader / pair around after the book entry is gone.
    #[test]
    fn build_recovery_settlement_matches_resolves_full_fills_after_book_removal() {
        let base = Address::repeat_byte(0xAA);
        let quote = Address::repeat_byte(0xBB);
        let mut pair_tokens = HashMap::new();
        pair_tokens.insert("ETH/USDC".to_string(), PairConfig::new(base, quote));

        let bid_id = Uuid::new_v4();
        let ask_id = Uuid::new_v4();
        let mut placed = HashMap::new();
        placed.insert(
            bid_id,
            order_at(bid_id, "ETH/USDC", Side::Buy, Address::repeat_byte(1)),
        );
        placed.insert(
            ask_id,
            order_at(ask_id, "ETH/USDC", Side::Sell, Address::repeat_byte(2)),
        );

        // EngineState book is empty — mirrors the post-apply_fill state
        // recovery would leave behind for full-fill matches. Snapshot must
        // still resolve them.
        let _empty_book = EngineState::new();

        let matches = vec![dp_auction::Match {
            bid: Fill {
                order_id: bid_id,
                size: Decimal::ONE,
            },
            ask: Fill {
                order_id: ask_id,
                size: Decimal::ONE,
            },
            price: Decimal::new(1500, 0),
            size: Decimal::ONE,
        }];
        let out = build_recovery_settlement_matches(&matches, &placed, &pair_tokens);
        assert_eq!(out.len(), 1, "snapshot must survive book removal");
    }

    fn placed_event(seq: u64) -> dp_event::Event {
        dp_event::Event {
            seq,
            event_type: EventType::OrderPlaced,
            timestamp: Utc::now(),
            data: EventData::OrderPlaced {
                order_id: Uuid::new_v4(),
                commitment: vec![],
                proof: vec![],
                ciphertext: vec![],
                salt_nonce: vec![0u8; 32],
            },
        }
    }

    #[test]
    fn continuous_after_empty_tail_is_ok() {
        let store = MemStore::new();
        // No events at all — nothing to replay, continuity holds.
        assert!(event_log_continuous_after(&store, 0).unwrap());
        assert!(event_log_continuous_after(&store, 99).unwrap());
    }

    #[test]
    fn continuous_after_next_is_exactly_seq_plus_one() {
        let store = MemStore::new();
        let mut evs = vec![placed_event(0), placed_event(0), placed_event(0)];
        store.append(&mut evs).unwrap(); // gets seq 1, 2, 3
                                         // after_seq=2 → first event after is seq 3 = 2+1 → continuous
        assert!(event_log_continuous_after(&store, 2).unwrap());
    }

    #[test]
    fn continuous_after_detects_gap() {
        let store = MemStore::new();
        let mut evs = vec![
            placed_event(0),
            placed_event(0),
            placed_event(0),
            placed_event(0),
        ];
        store.append(&mut evs).unwrap(); // seq 1..4
                                         // Compact away seq 1 and 2 (remove seq < 3).
        store.compact_before(3).unwrap();
        // after_seq=1 → first retained event is seq 3, not 2 → gap
        assert!(!event_log_continuous_after(&store, 1).unwrap());
    }
}
