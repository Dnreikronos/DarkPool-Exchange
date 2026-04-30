use std::collections::BTreeSet;

use dp_types::{Fill, Order};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuctionResult {
    pub auction_id: Uuid,
    pub pair: String,
    pub clearing_price: Decimal,
    pub matched_volume: Decimal,
    pub matches: Vec<Match>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub bid: Fill,
    pub ask: Fill,
    pub price: Decimal,
    pub size: Decimal,
}

#[must_use]
pub fn run(auction_id: Uuid, pair: &str, bids: &[Order], asks: &[Order]) -> Option<AuctionResult> {
    if bids.is_empty() || asks.is_empty() {
        return None;
    }

    let mut bids = bids.to_vec();
    let mut asks = asks.to_vec();
    bids.sort_by_key(|o| std::cmp::Reverse(o.price));
    asks.sort_by_key(|o| o.price);

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
    ((lo + hi) / Decimal::from(2)).normalize()
}

fn cumulative_volume(orders: &[Order], pred: impl Fn(&Order) -> bool) -> Decimal {
    orders
        .iter()
        .filter(|o| pred(o))
        .map(|o| o.remaining_size)
        .sum()
}

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

            if bid.commitment_key == ask.commitment_key {
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

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dp_types::Side;

    fn new_order(side: Side, price: i64, size: i64) -> Order {
        Order {
            id: Uuid::new_v4(),
            pair: "TEST/USD".to_string(),
            side,
            price: Decimal::from(price),
            size: Decimal::from(size),
            remaining_size: Decimal::from(size),
            commitment_key: Uuid::new_v4().to_string(),
            encrypted_payload: vec![],
            submitted_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
        }
    }

    fn test_auction_id() -> Uuid {
        Uuid::nil()
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

    #[test]
    fn self_match_prevention() {
        let bid = new_order(Side::Buy, 1800, 10);
        let mut ask = new_order(Side::Sell, 1790, 10);
        ask.commitment_key = bid.commitment_key.clone();

        assert!(run(test_auction_id(), "TEST/USD", &[bid], &[ask]).is_none());
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
        let bids = vec![
            new_order(Side::Buy, 110, 10),
            new_order(Side::Buy, 100, 20),
        ];
        let asks = vec![
            new_order(Side::Sell, 90, 10),
            new_order(Side::Sell, 100, 10),
            new_order(Side::Sell, 110, 10),
        ];

        let result = run(test_auction_id(), "TEST/USD", &bids, &asks).expect("expected result");
        assert_eq!(result.clearing_price, Decimal::from(100));
        assert_eq!(result.matched_volume, Decimal::from(20));
    }
}
