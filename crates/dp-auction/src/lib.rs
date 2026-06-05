use std::collections::BTreeSet;

use dp_types::{Fill, Order};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Decimal places the clearing price is quantised to. Must match
/// `dp_zk::encoding::DECIMAL_SCALE`: the ZK scalar encoder rejects any value
/// carrying more precision than this, so a clearing price beyond it could
/// never settle on-chain. Quantising here keeps the auction from emitting a
/// price the encoder will later reject after the matches were already
/// recorded. Kept as a local literal rather than depending on
/// `dp-zk` (which pulls in arkworks) for one constant; the two are pinned
/// together by a guard test in `dp-engine` (the only crate depending on
/// both), so drift in either becomes a test failure instead of a silent
/// trading halt. Public solely so that guard can compare against it.
pub const CLEARING_PRICE_DP: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuctionResult {
    pub auction_id: Uuid,
    pub pair: String,
    pub clearing_price: Decimal,
    pub matched_volume: Decimal,
    pub matches: Vec<Match>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub bid: Fill,
    pub ask: Fill,
    pub price: Decimal,
    pub size: Decimal,
}

/// Run one batch auction over the orders for `pair` and return the matches,
/// or `None` if the book does not cross.
///
/// Matching is **strict price-time priority** under the deterministic total
/// order `(price, seq)`: best price first, ties broken by the monotonic
/// placement `seq` (earliest placement wins). That ordering is established
/// here, inside `run`, rather than relying on the caller passing pre-sorted
/// slices — so the result is identical regardless of input order, and
/// identical between live execution and event-log replay. `seq` is unique per
/// order, making this a strict total order with no dependence on sort
/// stability. See ADR 0005 for why price-time (not pro-rata) and why `seq`
/// (not `submitted_at`).
///
/// Allocation is greedy under that order: the highest-priority order on each
/// side is filled as far as the opposing liquidity allows before the next is
/// considered. There is no pro-rata split.
#[must_use]
pub fn run(auction_id: Uuid, pair: &str, bids: &[Order], asks: &[Order]) -> Option<AuctionResult> {
    if bids.is_empty() || asks.is_empty() {
        return None;
    }

    let mut bids = bids.to_vec();
    let mut asks = asks.to_vec();
    // Total order: best price first, then earliest placement (seq asc). `seq`
    // is unique per order, so this fully determines priority without relying
    // on the caller's input order or on sort stability.
    bids.sort_by(|a, b| b.price.cmp(&a.price).then(a.seq.cmp(&b.seq)));
    asks.sort_by(|a, b| a.price.cmp(&b.price).then(a.seq.cmp(&b.seq)));

    if bids[0].price < asks[0].price {
        return None;
    }

    let clearing_price = compute_clearing_price(&bids, &asks);
    if clearing_price <= Decimal::ZERO {
        return None;
    }

    let matches = match_orders(&bids, &asks, clearing_price);
    if matches.is_empty() {
        return None;
    }

    let matched_volume = matches.iter().map(|m| m.size).sum();

    Some(AuctionResult {
        auction_id,
        pair: pair.to_string(),
        clearing_price,
        matched_volume,
        matches,
    })
}

fn compute_clearing_price(bids: &[Order], asks: &[Order]) -> Decimal {
    let mut candidates = BTreeSet::new();
    for b in bids {
        candidates.insert(b.price);
    }
    for a in asks {
        candidates.insert(a.price);
    }

    let mut best_volume = Decimal::ZERO;
    let mut tied_prices: Vec<Decimal> = Vec::new();

    for &p in &candidates {
        let bid_vol = cumulative_volume(bids, |o| o.price >= p);
        let ask_vol = cumulative_volume(asks, |o| o.price <= p);
        let matched = bid_vol.min(ask_vol);

        if matched > best_volume {
            best_volume = matched;
            tied_prices = vec![p];
        } else if matched == best_volume && matched > Decimal::ZERO {
            tied_prices.push(p);
        }
    }

    if tied_prices.is_empty() {
        return Decimal::ZERO;
    }

    let lo = tied_prices[0];
    let hi = tied_prices[tied_prices.len() - 1];
    // Halving the midpoint of two prices can introduce one extra decimal
    // place (e.g. 0.00000001 and 0.00000002 → 0.000000015). The ZK encoder
    // rejects anything beyond `CLEARING_PRICE_DP` dp, which previously let the
    // engine record matches it could never settle. Round the
    // midpoint to `CLEARING_PRICE_DP` dp so the clearing price is always
    // encodable; the rounded value stays within `[lo, hi]`, both of which are
    // volume-maximising clearing prices, so the matched set is unchanged.
    ((lo + hi) / Decimal::from(2))
        .round_dp(CLEARING_PRICE_DP)
        .normalize()
}

fn cumulative_volume(orders: &[Order], pred: impl Fn(&Order) -> bool) -> Decimal {
    orders
        .iter()
        .filter(|o| pred(o))
        .map(|o| o.remaining_size)
        .sum()
}

/// Greedy fill under strict price-time priority. `bids`/`asks` arrive already
/// in `(price, seq)` order (established in [`run`]); each order is matched in
/// that order against the opposing side, highest priority first, until its
/// remaining size is exhausted. Self-trades (orders from the same verified
/// `trader` address) are skipped.
fn match_orders(bids: &[Order], asks: &[Order], price: Decimal) -> Vec<Match> {
    let mut eligible_bids: Vec<Order> = bids
        .iter()
        .filter(|b| b.price >= price && b.remaining_size > Decimal::ZERO)
        .cloned()
        .collect();

    let mut eligible_asks: Vec<Order> = asks
        .iter()
        .filter(|a| a.price <= price && a.remaining_size > Decimal::ZERO)
        .cloned()
        .collect();

    let mut matches = Vec::new();

    for bid in &mut eligible_bids {
        for ask in &mut eligible_asks {
            if bid.remaining_size <= Decimal::ZERO {
                break;
            }

            if ask.remaining_size <= Decimal::ZERO {
                continue;
            }

            // Self-trade prevention keys on the verified on-chain `trader`
            // address, not the client-chosen per-order `commitment_key`
            // (#168). The commitment_key is trader-supplied, so keying on it
            // let one trader wash-trade across two keys and wrongly blocked
            // two distinct traders who happened to reuse a key. `trader` is
            // the address the engine verified against the submitting caller
            // (see `dp-engine` `place_order`), so it cannot be spoofed to
            // dodge — or forge — a self-cross.
            if bid.trader == ask.trader {
                continue;
            }

            let fill_size = bid.remaining_size.min(ask.remaining_size);

            matches.push(Match {
                bid: Fill {
                    order_id: bid.id,
                    size: fill_size,
                },
                ask: Fill {
                    order_id: ask.id,
                    size: fill_size,
                },
                price,
                size: fill_size,
            });

            bid.remaining_size -= fill_size;
            ask.remaining_size -= fill_size;
        }
    }

    // Conservation invariant: every match debits the bid and credits the ask
    // by the same `fill_size` and prices at the clearing price, so volume is
    // conserved leg-for-leg (Σ bid fills == Σ ask fills) and no fill happens
    // off the clearing price. True by construction here; asserted in
    // debug/test builds to catch any regression, with no release-build cost.
    debug_assert!(
        matches
            .iter()
            .all(|m| m.price == price && m.bid.size == m.ask.size),
        "every fill must price at the clearing price with equal bid/ask legs"
    );
    debug_assert_eq!(
        matches.iter().map(|m| m.bid.size).sum::<Decimal>(),
        matches.iter().map(|m| m.ask.size).sum::<Decimal>(),
        "sum of bid fills must equal sum of ask fills"
    );

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dp_types::Side;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Each freshly built order gets a distinct `trader` address. Self-trade
    /// prevention keys on `trader` (#168), so leaving every order at
    /// `Address::ZERO` would make every two-sided test look like a self-cross
    /// and match nothing. Tests that *want* a self-trade copy one order's
    /// `trader` onto the other explicitly.
    fn unique_trader() -> alloy_primitives::Address {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 20];
        bytes[12..].copy_from_slice(&n.to_be_bytes());
        alloy_primitives::Address::from(bytes)
    }

    fn new_order(side: Side, price: i64, size: i64) -> Order {
        Order {
            id: Uuid::new_v4(),
            trader: unique_trader(),
            pair: "TEST/USD".to_string(),
            side,
            price: Decimal::from(price),
            size: Decimal::from(size),
            remaining_size: Decimal::from(size),
            commitment_key: Uuid::new_v4().to_string(),
            encrypted_payload: vec![],
            submitted_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            seq: 0,
        }
    }

    /// Like [`new_order`] but takes a `Decimal` price so tests can exercise
    /// sub-integer (and sub-tick) prices.
    fn new_order_px(side: Side, price: Decimal, size: i64) -> Order {
        Order {
            price,
            ..new_order(side, 0, size)
        }
    }

    fn test_auction_id() -> Uuid {
        Uuid::nil()
    }

    /// When the midpoint of two tied clearing prices would carry
    /// more than 8 dp (odd last digit halved), it must be quantised down to a
    /// price the ZK encoder accepts — and stay within `[lo, hi]`, both of
    /// which are valid volume-maximising clearing prices.
    #[test]
    fn clearing_price_quantized_to_encodable_precision() {
        // Naive midpoint of these is 0.000000015 (9 dp), which
        // `decimal_to_scalar` rejects — silently dropping a batch whose
        // matches were already recorded.
        let lo = Decimal::new(1, 8); // 0.00000001
        let hi = Decimal::new(2, 8); // 0.00000002
        let bids = vec![new_order_px(Side::Buy, hi, 10)];
        let asks = vec![new_order_px(Side::Sell, lo, 10)];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");

        assert!(
            result.clearing_price.scale() <= 8,
            "clearing price {} must encode within 8 dp",
            result.clearing_price
        );
        assert!(
            result.clearing_price >= lo && result.clearing_price <= hi,
            "quantised clearing price {} must stay within [{lo}, {hi}]",
            result.clearing_price
        );
        assert!(
            result
                .matches
                .iter()
                .all(|m| m.price == result.clearing_price),
            "every fill must price at the quantised clearing price"
        );
    }

    #[test]
    fn basic_match() {
        let bids = vec![new_order(Side::Buy, 1800, 10)];
        let asks = vec![new_order(Side::Sell, 1790, 10)];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");
        assert_eq!(result.auction_id, test_auction_id());
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matched_volume, Decimal::from(10));
    }

    #[test]
    fn no_crossing() {
        let bids = vec![new_order(Side::Buy, 1700, 10)];
        let asks = vec![new_order(Side::Sell, 1800, 10)];

        assert!(run(test_auction_id(), "TEST/USD", &bids, &asks).is_none());
    }

    #[test]
    fn partial_fill() {
        let bids = vec![new_order(Side::Buy, 1800, 10)];
        let asks = vec![new_order(Side::Sell, 1790, 4)];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");
        assert_eq!(result.matched_volume, Decimal::from(4));
    }

    #[test]
    fn multiple_bids_and_asks() {
        let bids = vec![
            new_order(Side::Buy, 1810, 5),
            new_order(Side::Buy, 1800, 10),
            new_order(Side::Buy, 1795, 8),
        ];
        let asks = vec![
            new_order(Side::Sell, 1785, 6),
            new_order(Side::Sell, 1790, 4),
            new_order(Side::Sell, 1800, 10),
        ];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");
        assert!(result.clearing_price > Decimal::ZERO);
        assert!(result.matched_volume > Decimal::ZERO);
    }

    /// #168: a trader cannot cross their own bid and ask, even when the two
    /// orders carry *different* `commitment_key`s — the wash-trade path the
    /// old commitment_key-based check let through. Prevention keys on the
    /// verified `trader` address instead.
    #[test]
    fn self_match_prevention() {
        let bid = new_order(Side::Buy, 1800, 10);
        let mut ask = new_order(Side::Sell, 1790, 10);
        ask.trader = bid.trader;
        // Distinct keys prove the block is driven by `trader`, not the key.
        assert_ne!(bid.commitment_key, ask.commitment_key);

        assert!(run(test_auction_id(), "TEST/USD", &[bid], &[ask]).is_none());
    }

    /// #168: two *distinct* traders who happen to reuse the same
    /// `commitment_key` must still match — the false-positive the old check
    /// produced. Prevention no longer keys on the shared key.
    #[test]
    fn distinct_traders_sharing_commitment_key_still_match() {
        let bid = new_order(Side::Buy, 1800, 10);
        let mut ask = new_order(Side::Sell, 1790, 10);
        ask.commitment_key = bid.commitment_key.clone();
        assert_ne!(bid.trader, ask.trader);

        let result = run(test_auction_id(), "TEST/USD", &[bid], &[ask]).expect("expected match");
        assert_eq!(result.matched_volume, Decimal::from(10));
    }

    #[test]
    fn empty_side() {
        let bids = vec![new_order(Side::Buy, 1800, 10)];
        let asks: Vec<Order> = vec![];

        assert!(run(test_auction_id(), "TEST/USD", &bids, &asks).is_none());

        let bids: Vec<Order> = vec![];
        let asks = vec![new_order(Side::Sell, 1790, 10)];

        assert!(run(test_auction_id(), "TEST/USD", &bids, &asks).is_none());
    }

    #[test]
    fn clearing_price_maximizes_volume() {
        let bids = vec![new_order(Side::Buy, 110, 10), new_order(Side::Buy, 100, 20)];
        let asks = vec![
            new_order(Side::Sell, 90, 10),
            new_order(Side::Sell, 100, 10),
            new_order(Side::Sell, 110, 10),
        ];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");
        assert_eq!(result.clearing_price, Decimal::from(100));
        assert_eq!(result.matched_volume, Decimal::from(20));
    }

    /// Issue #161: every auction must conserve volume leg-for-leg and price
    /// every fill at the single clearing price.
    #[test]
    fn fills_conserve_volume_and_clear_at_one_price() {
        let bids = vec![
            new_order(Side::Buy, 1810, 5),
            new_order(Side::Buy, 1800, 10),
        ];
        let asks = vec![
            new_order(Side::Sell, 1790, 8),
            new_order(Side::Sell, 1795, 4),
        ];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");

        let bid_total: Decimal = result.matches.iter().map(|m| m.bid.size).sum();
        let ask_total: Decimal = result.matches.iter().map(|m| m.ask.size).sum();

        assert_eq!(bid_total, ask_total, "Σ bid fills must equal Σ ask fills");
        assert_eq!(
            bid_total, result.matched_volume,
            "matched_volume must equal the conserved fill total"
        );
        assert!(
            result
                .matches
                .iter()
                .all(|m| m.price == result.clearing_price),
            "every fill must price at the clearing price"
        );
    }

    /// Issue #161: among same-price orders the lower `seq` (earlier placement)
    /// fills first; the marginal order takes the remainder. This is the
    /// documented price-time priority rule.
    #[test]
    fn strict_price_time_priority_breaks_ties_by_seq() {
        let mut bid = new_order(Side::Buy, 1790, 8);
        bid.seq = 1;
        let mut ask_early = new_order(Side::Sell, 1790, 6);
        ask_early.seq = 2;
        let mut ask_late = new_order(Side::Sell, 1790, 6);
        ask_late.seq = 3;

        let result = run(
            test_auction_id(),
            "TEST/USD",
            &[bid],
            &[ask_early.clone(), ask_late.clone()],
        )
        .expect("expected result");

        let early_fill: Decimal = result
            .matches
            .iter()
            .filter(|m| m.ask.order_id == ask_early.id)
            .map(|m| m.ask.size)
            .sum();
        let late_fill: Decimal = result
            .matches
            .iter()
            .filter(|m| m.ask.order_id == ask_late.id)
            .map(|m| m.ask.size)
            .sum();

        assert_eq!(
            early_fill,
            Decimal::from(6),
            "earlier seq fills fully first"
        );
        assert_eq!(late_fill, Decimal::from(2), "later seq takes the remainder");
    }

    /// Issue #161: the result is a function of `(price, seq)` alone — feeding
    /// the same orders in a different input order must produce an identical
    /// matching. Guards against the old reliance on caller sort-stability.
    #[test]
    fn result_independent_of_input_order() {
        let mut b1 = new_order(Side::Buy, 1800, 5);
        b1.seq = 10;
        let mut b2 = new_order(Side::Buy, 1800, 5);
        b2.seq = 11;
        let mut a1 = new_order(Side::Sell, 1790, 5);
        a1.seq = 12;
        let mut a2 = new_order(Side::Sell, 1790, 5);
        a2.seq = 13;

        let forward = run(
            test_auction_id(),
            "TEST/USD",
            &[b1.clone(), b2.clone()],
            &[a1.clone(), a2.clone()],
        )
        .expect("expected result");
        let reversed = run(
            test_auction_id(),
            "TEST/USD",
            &[b2.clone(), b1.clone()],
            &[a2.clone(), a1.clone()],
        )
        .expect("expected result");

        assert_eq!(forward.matches, reversed.matches);
        assert_eq!(forward.clearing_price, reversed.clearing_price);
        assert_eq!(forward.matched_volume, reversed.matched_volume);
    }
}
