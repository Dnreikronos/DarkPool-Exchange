use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use dp_auction::Match;
use dp_book::OrderBook;
use dp_settlement::SettlementMatch;
use uuid::Uuid;

use crate::{
    DEFAULT_MAX_BACKOFF, DEFAULT_MIN_BACKOFF, DEFAULT_ORDER_TTL, DEFAULT_SUBMIT_TIMEOUT,
};

#[derive(Clone, Debug)]
pub struct PairConfig {
    pub base_token: Address,
    pub quote_token: Address,
}

#[derive(Clone, Debug)]
pub struct PendingBatch {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    pub matches: Vec<Match>,
    pub settlement_matches: Vec<SettlementMatch>,
    pub proof: Vec<u8>,
    pub public_inputs: [U256; 6],
    pub attempts: u32,
    pub next_attempt: Option<Instant>,
    pub submitting: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AuctionExecutedRecord {
    pub auction_id: Uuid,
    pub pair: String,
    pub clearing_price: rust_decimal::Decimal,
    pub matched_volume: rust_decimal::Decimal,
    pub match_count: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub(crate) struct EngineState {
    pub book: OrderBook,
    pub pairs: HashSet<String>,
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
            pairs: HashSet::new(),
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
        self.pairs.clear();
        self.auction_log.clear();
        self.pending_batches.clear();
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
}
