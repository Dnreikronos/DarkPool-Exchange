use std::collections::HashMap;
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use dp_auction::Match;
use dp_book::{BookSnapshot, OrderBook};
use dp_settlement::SettlementMatch;
use dp_types::Order;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DEFAULT_MAX_BACKOFF, DEFAULT_MIN_BACKOFF, DEFAULT_ORDER_TTL, DEFAULT_SUBMIT_TIMEOUT};

/// Why resolving a settlement row from a match + snapshots failed.
/// Recovery treats either case as "skip the row" (orphan log entries);
/// the live tick path treats either as an internal invariant violation
/// and converts to `EngineError::WitnessOrderMissing` / `PairNotConfigured`.
pub(crate) enum SettlementResolveErr {
    OrderMissing(Uuid),
    PairMissing(String),
}

/// Build one settlement row from a match by resolving trader / pair
/// metadata in the supplied snapshots. Centralising the lookup keeps the
/// recovery and live-tick paths in lockstep (they previously diverged —
/// recovery used `book.find_order` and missed full-fill orders).
pub(crate) fn try_build_settlement_row(
    m: &Match,
    orders: &HashMap<Uuid, Order>,
    pairs: &HashMap<String, PairConfig>,
) -> Result<SettlementMatch, SettlementResolveErr> {
    let bid = orders
        .get(&m.bid.order_id)
        .ok_or(SettlementResolveErr::OrderMissing(m.bid.order_id))?;
    let ask = orders
        .get(&m.ask.order_id)
        .ok_or(SettlementResolveErr::OrderMissing(m.ask.order_id))?;
    let cfg = pairs
        .get(&bid.pair)
        .ok_or_else(|| SettlementResolveErr::PairMissing(bid.pair.clone()))?;
    Ok(SettlementMatch {
        bid_order_id: m.bid.order_id,
        ask_order_id: m.ask.order_id,
        bid_trader: bid.trader,
        ask_trader: ask.trader,
        base_token: cfg.base_token,
        quote_token: cfg.quote_token,
        price: m.price,
        size: m.size,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairStatus {
    Active,
    Suspended,
    Delisted,
}

impl PairStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, PairStatus::Active)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairConfig {
    pub base_token: Address,
    pub quote_token: Address,
    pub min_order_size: Decimal,
    pub tick_size: Decimal,
    /// Reserved for future per-pair tick cadence overrides. The auction
    /// loop currently uses the engine-wide interval — this field is
    /// captured and replayed today so a later patch can honour it without
    /// a schema change.
    pub auction_interval: Option<Duration>,
    pub status: PairStatus,
}

impl PairConfig {
    /// Construct a config with permissive matching parameters (zero
    /// min_order_size, zero tick_size = no tick check) and `Active`
    /// status. Useful for tests and the in-memory dev seed; production
    /// configs should set explicit guards via the admin API.
    pub fn new(base_token: Address, quote_token: Address) -> Self {
        Self {
            base_token,
            quote_token,
            min_order_size: Decimal::ZERO,
            tick_size: Decimal::ZERO,
            auction_interval: None,
            status: PairStatus::Active,
        }
    }
}

impl Default for PairConfig {
    fn default() -> Self {
        Self::new(Address::ZERO, Address::ZERO)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingBatch {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    pub matches: Vec<Match>,
    pub settlement_matches: Vec<SettlementMatch>,
    pub proof: Vec<u8>,
    pub public_inputs: [U256; 6],
    pub attempts: u32,
    /// `Instant` has no stable serialised representation; the retry
    /// scheduler will compute a fresh attempt time the next time the
    /// batch is considered, so we drop the wall-clock pointer on
    /// snapshot round-trip and let the recover path fall back to the
    /// default `None`.
    #[serde(skip)]
    pub next_attempt: Option<Instant>,
    /// In-flight submission flag is a transient runtime guard — after a
    /// snapshot-and-restart there is no in-flight work, so reset to
    /// `false` instead of trying to round-trip it.
    #[serde(skip)]
    pub submitting: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AuctionExecutedRecord {
    pub auction_id: Uuid,
    pub pair: String,
    pub clearing_price: rust_decimal::Decimal,
    pub matched_volume: rust_decimal::Decimal,
    pub match_count: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Persistable subset of [`EngineState`]. Captures every piece of state
/// that recovery rebuilds from the event stream so a snapshot + the
/// event tail past `applied_seq` is equivalent to a full replay. The
/// runtime-only fields (`submit_timeout`, backoff bounds, `recovered`
/// flag) are intentionally excluded — they come from `Config`, not the
/// event log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SerializableState {
    pub book: BookSnapshot,
    pub pair_tokens: HashMap<String, PairConfig>,
    pub auction_log: Vec<AuctionExecutedRecord>,
    pub pending_batches: HashMap<Uuid, PendingBatch>,
}

pub(crate) struct EngineState {
    pub book: OrderBook,
    /// Authoritative pair registry. A pair is "known" iff it has an entry
    /// here; status is read from the `PairConfig`. The registry survives
    /// projection resets so it can be replayed from `PairRegistered` /
    /// `PairSuspended` / `PairDelisted` events.
    pub pair_tokens: HashMap<String, PairConfig>,
    pub auction_log: Vec<AuctionExecutedRecord>,
    pub pending_batches: HashMap<Uuid, PendingBatch>,
    pub submit_timeout: Duration,
    pub min_backoff: Duration,
    pub max_backoff: Duration,
    pub default_ttl: Duration,
    pub recovered: bool,
}

impl EngineState {
    pub fn new() -> Self {
        Self {
            book: OrderBook::new(),
            pair_tokens: HashMap::new(),
            auction_log: Vec::new(),
            pending_batches: HashMap::new(),
            submit_timeout: DEFAULT_SUBMIT_TIMEOUT,
            min_backoff: DEFAULT_MIN_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            default_ttl: DEFAULT_ORDER_TTL,
            recovered: false,
        }
    }

    pub fn reset_projection(&mut self) {
        self.book = OrderBook::new();
        self.pair_tokens.clear();
        self.auction_log.clear();
        self.pending_batches.clear();
    }

    pub fn pair_config(&self, pair: &str) -> Option<&PairConfig> {
        self.pair_tokens.get(pair)
    }

    /// Capture the persistable subset of state into a [`SerializableState`].
    /// Holds no locks of its own (caller owns the `Mutex<EngineState>`
    /// guard); clones every contained collection so the resulting value
    /// is independent of the live state.
    pub(crate) fn to_serializable(&self) -> SerializableState {
        SerializableState {
            book: self.book.to_snapshot(),
            pair_tokens: self.pair_tokens.clone(),
            auction_log: self.auction_log.clone(),
            pending_batches: self.pending_batches.clone(),
        }
    }

    /// Replace the projection-derived fields with the contents of
    /// `snap`. The runtime-only fields (timeouts, backoff, `recovered`)
    /// are untouched so the engine's `Config`-supplied values win.
    pub(crate) fn restore_from_serializable(&mut self, snap: SerializableState) {
        self.book.restore_from(snap.book);
        self.pair_tokens = snap.pair_tokens;
        self.auction_log = snap.auction_log;
        self.pending_batches = snap.pending_batches;
    }
}

pub(crate) fn compute_backoff(min: Duration, max: Duration, attempts: u32) -> Duration {
    if min.is_zero() {
        return Duration::ZERO;
    }
    let attempts = attempts.max(1);
    let shift = attempts.saturating_sub(1).min(31);
    let nanos = min.as_nanos() << shift;
    if nanos > max.as_nanos() {
        return max;
    }
    Duration::from_nanos(nanos.min(u64::MAX as u128) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_min_zero_returns_zero() {
        assert_eq!(
            compute_backoff(Duration::ZERO, Duration::from_secs(60), 5),
            Duration::ZERO
        );
    }

    #[test]
    fn backoff_first_attempt_is_min() {
        assert_eq!(
            compute_backoff(Duration::from_secs(1), Duration::from_secs(60), 1),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn backoff_doubles_each_attempt() {
        let min = Duration::from_secs(1);
        let max = Duration::from_secs(60);
        assert_eq!(compute_backoff(min, max, 2), Duration::from_secs(2));
        assert_eq!(compute_backoff(min, max, 3), Duration::from_secs(4));
        assert_eq!(compute_backoff(min, max, 4), Duration::from_secs(8));
    }

    #[test]
    fn backoff_clamps_to_max() {
        let min = Duration::from_secs(1);
        let max = Duration::from_secs(60);
        assert_eq!(compute_backoff(min, max, 100), max);
    }

    #[test]
    fn pair_status_predicates() {
        assert!(PairStatus::Active.is_active());
        assert!(!PairStatus::Suspended.is_active());
        assert!(!PairStatus::Delisted.is_active());
    }
}
