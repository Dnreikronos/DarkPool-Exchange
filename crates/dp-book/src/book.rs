use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dp_event::{Event, EventData, EventError, Store};
use dp_types::{Fill, Order, Side};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct PerPairBook {
    bids: HashMap<Uuid, Order>,
    asks: HashMap<Uuid, Order>,
}

impl PerPairBook {
    fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Inner {
    books: HashMap<String, PerPairBook>,
    seq: u64,
}

/// Persistable snapshot of the order book — every per-pair sub-book plus
/// the highest event-sequence number applied. The engine writes this
/// inside its periodic snapshot envelope and restores it on boot before
/// replaying events past `seq`. Public so the engine crate can wrap it
/// inside its serializable engine-state struct, but the fields themselves
/// are implementation detail.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookSnapshot {
    inner: Inner,
}

pub struct OrderBook {
    inner: RwLock<Inner>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                books: HashMap::new(),
                seq: 0,
            }),
        }
    }

    pub fn replay(&self, store: &dyn Store) -> Result<(), EventError> {
        const BATCH: usize = 1024;
        let mut inner = self.inner.write();
        loop {
            let events = store.read_from(inner.seq, BATCH)?;
            if events.is_empty() {
                break;
            }
            for event in &events {
                inner.apply(event);
            }
        }
        Ok(())
    }

    /// Clone the current book state into a serialisable snapshot. Holds
    /// the read lock only for the clone — the snapshot is independent
    /// of the live book after this returns.
    pub fn to_snapshot(&self) -> BookSnapshot {
        BookSnapshot {
            inner: self.inner.read().clone(),
        }
    }

    /// Replace the live book state with `snap`. Takes the write lock for
    /// the swap; subsequent applies / inserts pick up the restored
    /// state. Used by the engine's recover path immediately after
    /// loading a snapshot, before replaying events past `applied_seq`.
    pub fn restore_from(&self, snap: BookSnapshot) {
        let mut inner = self.inner.write();
        *inner = snap.inner;
    }

    /// Highest event sequence number applied to the book. Used by the
    /// recover path to choose the starting seq for the post-snapshot
    /// event replay.
    pub fn applied_seq(&self) -> u64 {
        self.inner.read().seq
    }

    pub fn apply(&self, event: &Event) {
        self.inner.write().apply(event);
    }

    pub fn insert_order(&self, order: Order) {
        let mut inner = self.inner.write();
        let pair = order.pair.clone();
        let book = inner.books.entry(pair).or_default();
        match order.side {
            Side::Buy => book.bids.insert(order.id, order),
            Side::Sell => book.asks.insert(order.id, order),
        };
    }

    /// Sorted bids for a single pair (price desc, then submission time asc).
    pub fn bids(&self, pair: &str) -> Vec<Order> {
        let inner = self.inner.read();
        let Some(book) = inner.books.get(pair) else {
            return Vec::new();
        };
        let mut out: Vec<Order> = book.bids.values().cloned().collect();
        out.sort_by(|a, b| {
            b.price
                .cmp(&a.price)
                .then(a.submitted_at.cmp(&b.submitted_at))
        });
        out
    }

    /// Sorted asks for a single pair (price asc, then submission time asc).
    pub fn asks(&self, pair: &str) -> Vec<Order> {
        let inner = self.inner.read();
        let Some(book) = inner.books.get(pair) else {
            return Vec::new();
        };
        let mut out: Vec<Order> = book.asks.values().cloned().collect();
        out.sort_by(|a, b| {
            a.price
                .cmp(&b.price)
                .then(a.submitted_at.cmp(&b.submitted_at))
        });
        out
    }

    /// Snapshot of every active order across every sub-book. Used by the
    /// expiry sweep and other cross-pair maintenance paths.
    pub fn iter_all(&self) -> Vec<Order> {
        let inner = self.inner.read();
        let mut out = Vec::new();
        for book in inner.books.values() {
            out.extend(book.bids.values().cloned());
            out.extend(book.asks.values().cloned());
        }
        out
    }

    pub fn pairs(&self) -> Vec<String> {
        let inner = self.inner.read();
        inner.books.keys().cloned().collect()
    }

    pub fn collect_expired(&self, now: DateTime<Utc>) -> Vec<Event> {
        let inner = self.inner.read();
        let default_time = DateTime::<Utc>::default();
        let mut expired = Vec::new();

        for book in inner.books.values() {
            for order in book.bids.values().chain(book.asks.values()) {
                if order.expires_at != default_time && order.expires_at <= now {
                    expired.push(Event {
                        seq: 0,
                        event_type: dp_types::EventType::OrderExpired,
                        timestamp: DateTime::default(),
                        data: EventData::OrderExpired { order_id: order.id },
                    });
                }
            }
        }
        expired
    }

    pub fn find_order(&self, id: Uuid) -> Option<Order> {
        let inner = self.inner.read();
        for book in inner.books.values() {
            if let Some(o) = book.bids.get(&id).or_else(|| book.asks.get(&id)) {
                return Some(o.clone());
            }
        }
        None
    }

    pub fn has_order(&self, id: Uuid) -> bool {
        let inner = self.inner.read();
        inner
            .books
            .values()
            .any(|b| b.bids.contains_key(&id) || b.asks.contains_key(&id))
    }

    pub fn active_order_count(&self) -> usize {
        let inner = self.inner.read();
        inner
            .books
            .values()
            .map(|b| b.bids.len() + b.asks.len())
            .sum()
    }

    pub fn len_for(&self, pair: &str) -> usize {
        let inner = self.inner.read();
        inner
            .books
            .get(pair)
            .map(|b| b.bids.len() + b.asks.len())
            .unwrap_or(0)
    }

    /// IDs of every order on `pair`, bids then asks. Order within each
    /// side is the underlying HashMap's iteration order, which is fine
    /// for callers that just need the set (e.g. delist cancels). Avoids
    /// the full-Order clone that `bids()` + `asks()` would do for a
    /// caller that only needs the ids.
    pub fn order_ids_for(&self, pair: &str) -> Vec<Uuid> {
        let inner = self.inner.read();
        let Some(book) = inner.books.get(pair) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(book.bids.len() + book.asks.len());
        out.extend(book.bids.keys().copied());
        out.extend(book.asks.keys().copied());
        out
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

impl Inner {
    fn apply(&mut self, event: &Event) {
        if event.seq > self.seq {
            self.seq = event.seq;
        }
        match &event.data {
            EventData::OrderPlaced { .. } => {}
            EventData::OrderCancelled { order_id, .. } | EventData::OrderExpired { order_id } => {
                self.remove_order(*order_id);
            }
            EventData::OrderMatched { bid, ask, .. } => {
                self.apply_fill(bid);
                self.apply_fill(ask);
            }
            EventData::AuctionExecuted { .. }
            | EventData::BatchSubmitted { .. }
            | EventData::BatchConfirmed { .. }
            | EventData::BatchSettled { .. }
            | EventData::PairRegistered { .. }
            | EventData::PairSuspended { .. }
            | EventData::PairDelisted { .. } => {}
        }
    }

    fn remove_order(&mut self, order_id: Uuid) {
        let mut emptied: Option<String> = None;
        for (pair, book) in self.books.iter_mut() {
            if book.bids.remove(&order_id).is_some() || book.asks.remove(&order_id).is_some() {
                if book.is_empty() {
                    emptied = Some(pair.clone());
                }
                break;
            }
        }
        if let Some(p) = emptied {
            self.books.remove(&p);
        }
    }

    fn apply_fill(&mut self, fill: &Fill) {
        let mut emptied: Option<String> = None;
        for (pair, book) in self.books.iter_mut() {
            if let Some(order) = book.bids.get_mut(&fill.order_id) {
                order.remaining_size -= fill.size;
                if order.remaining_size <= Decimal::ZERO {
                    book.bids.remove(&fill.order_id);
                    if book.is_empty() {
                        emptied = Some(pair.clone());
                    }
                }
                break;
            }
            if let Some(order) = book.asks.get_mut(&fill.order_id) {
                order.remaining_size -= fill.size;
                if order.remaining_size <= Decimal::ZERO {
                    book.asks.remove(&fill.order_id);
                    if book.is_empty() {
                        emptied = Some(pair.clone());
                    }
                }
                break;
            }
        }
        if let Some(p) = emptied {
            self.books.remove(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use dp_event::MemStore;
    use dp_types::EventType;

    fn make_order(side: Side, price: i64, size: i64) -> Order {
        make_order_pair(side, price, size, "BTC-USD")
    }

    fn make_order_pair(side: Side, price: i64, size: i64, pair: &str) -> Order {
        Order {
            id: Uuid::new_v4(),
            trader: alloy_primitives::Address::ZERO,
            pair: pair.into(),
            side,
            price: Decimal::new(price, 0),
            size: Decimal::new(size, 0),
            remaining_size: Decimal::new(size, 0),
            commitment_key: String::new(),
            encrypted_payload: vec![],
            submitted_at: Utc::now(),
            expires_at: DateTime::default(),
        }
    }

    #[test]
    fn place_and_cancel() {
        let book = OrderBook::new();
        let order = make_order(Side::Buy, 100, 5);
        let oid = order.id;

        book.insert_order(order);
        assert_eq!(book.active_order_count(), 1);
        assert!(book.has_order(oid));

        book.apply(&Event {
            seq: 1,
            event_type: EventType::OrderCancelled,
            timestamp: Utc::now(),
            data: EventData::OrderCancelled {
                order_id: oid,
                reason: "user".into(),
            },
        });

        assert_eq!(book.active_order_count(), 0);
        assert!(!book.has_order(oid));
    }

    #[test]
    fn partial_fill() {
        let book = OrderBook::new();
        let bid = make_order(Side::Buy, 100, 10);
        let ask = make_order(Side::Sell, 100, 4);
        let bid_id = bid.id;
        let ask_id = ask.id;

        book.insert_order(bid);
        book.insert_order(ask);
        assert_eq!(book.active_order_count(), 2);

        book.apply(&Event {
            seq: 1,
            event_type: EventType::OrderMatched,
            timestamp: Utc::now(),
            data: EventData::OrderMatched {
                auction_id: Uuid::new_v4(),
                bid: Fill {
                    order_id: bid_id,
                    size: Decimal::new(4, 0),
                },
                ask: Fill {
                    order_id: ask_id,
                    size: Decimal::new(4, 0),
                },
                price: Decimal::new(100, 0),
                size: Decimal::new(4, 0),
            },
        });

        assert_eq!(book.active_order_count(), 1);
        let remaining = book.find_order(bid_id).unwrap();
        assert_eq!(remaining.remaining_size, Decimal::new(6, 0));
        assert!(!book.has_order(ask_id));
    }

    #[test]
    fn expiration_collection() {
        let book = OrderBook::new();
        let mut order = make_order(Side::Buy, 50, 1);
        order.expires_at = Utc::now() - Duration::seconds(10);
        let oid = order.id;

        book.insert_order(order);
        assert_eq!(book.active_order_count(), 1);

        let now = Utc::now();
        let expired_events = book.collect_expired(now);
        assert_eq!(expired_events.len(), 1);

        for e in &expired_events {
            book.apply(e);
        }
        assert_eq!(book.active_order_count(), 0);
        assert!(!book.has_order(oid));
    }

    #[test]
    fn replay_advances_seq() {
        let store = MemStore::new();
        let mut events = vec![Event {
            seq: 0,
            event_type: EventType::OrderPlaced,
            timestamp: DateTime::default(),
            data: EventData::OrderPlaced {
                order_id: Uuid::new_v4(),
                commitment: vec![1],
                proof: vec![2],
                ciphertext: vec![3],
                salt_nonce: vec![0u8; 32],
            },
        }];
        store.append(&mut events).unwrap();

        let book = OrderBook::new();
        book.replay(&store).unwrap();

        // OrderPlaced is no-op for book state (ciphertext only)
        assert_eq!(book.active_order_count(), 0);
        // But seq advanced
        assert_eq!(book.inner.read().seq, 1);
    }

    #[test]
    fn bids_sorted_price_desc_time_asc() {
        let book = OrderBook::new();
        let mut o1 = make_order(Side::Buy, 100, 1);
        o1.submitted_at = Utc::now() - Duration::seconds(10);
        let mut o2 = make_order(Side::Buy, 200, 1);
        o2.submitted_at = Utc::now();
        let mut o3 = make_order(Side::Buy, 200, 1);
        o3.submitted_at = Utc::now() - Duration::seconds(5);

        book.insert_order(o1.clone());
        book.insert_order(o2.clone());
        book.insert_order(o3.clone());

        let bids = book.bids("BTC-USD");
        assert_eq!(bids[0].id, o3.id); // price 200, earlier
        assert_eq!(bids[1].id, o2.id); // price 200, later
        assert_eq!(bids[2].id, o1.id); // price 100
    }

    #[test]
    fn asks_sorted_price_asc_time_asc() {
        let book = OrderBook::new();
        let mut o1 = make_order(Side::Sell, 300, 1);
        o1.submitted_at = Utc::now();
        let mut o2 = make_order(Side::Sell, 100, 1);
        o2.submitted_at = Utc::now() - Duration::seconds(5);
        let mut o3 = make_order(Side::Sell, 100, 1);
        o3.submitted_at = Utc::now();

        book.insert_order(o1.clone());
        book.insert_order(o2.clone());
        book.insert_order(o3.clone());

        let asks = book.asks("BTC-USD");
        assert_eq!(asks[0].id, o2.id); // price 100, earlier
        assert_eq!(asks[1].id, o3.id); // price 100, later
        assert_eq!(asks[2].id, o1.id); // price 300
    }

    #[test]
    fn pairs_are_isolated() {
        let book = OrderBook::new();
        let btc_bid = make_order_pair(Side::Buy, 50_000, 1, "BTC-USD");
        let eth_bid = make_order_pair(Side::Buy, 3_000, 5, "ETH-USDC");
        let eth_ask = make_order_pair(Side::Sell, 3_100, 2, "ETH-USDC");

        book.insert_order(btc_bid.clone());
        book.insert_order(eth_bid.clone());
        book.insert_order(eth_ask.clone());

        assert_eq!(book.active_order_count(), 3);
        assert_eq!(book.len_for("BTC-USD"), 1);
        assert_eq!(book.len_for("ETH-USDC"), 2);
        assert_eq!(book.len_for("OTHER"), 0);

        let btc_bids = book.bids("BTC-USD");
        assert_eq!(btc_bids.len(), 1);
        assert_eq!(btc_bids[0].id, btc_bid.id);

        let eth_bids = book.bids("ETH-USDC");
        assert_eq!(eth_bids.len(), 1);
        assert_eq!(eth_bids[0].id, eth_bid.id);

        let eth_asks = book.asks("ETH-USDC");
        assert_eq!(eth_asks.len(), 1);
        assert_eq!(eth_asks[0].id, eth_ask.id);

        // Cross-pair: BTC has no asks at all.
        assert!(book.asks("BTC-USD").is_empty());
    }

    #[test]
    fn cancel_removes_only_from_owning_subbook() {
        let book = OrderBook::new();
        let btc = make_order_pair(Side::Buy, 50_000, 1, "BTC-USD");
        let eth = make_order_pair(Side::Buy, 3_000, 5, "ETH-USDC");
        let btc_id = btc.id;
        let eth_id = eth.id;
        book.insert_order(btc);
        book.insert_order(eth);

        book.apply(&Event {
            seq: 1,
            event_type: EventType::OrderCancelled,
            timestamp: Utc::now(),
            data: EventData::OrderCancelled {
                order_id: btc_id,
                reason: "user".into(),
            },
        });

        assert!(!book.has_order(btc_id));
        assert!(book.has_order(eth_id));
        assert_eq!(book.len_for("BTC-USD"), 0);
        assert_eq!(book.len_for("ETH-USDC"), 1);
    }

    #[test]
    fn iter_all_unions_subbooks() {
        let book = OrderBook::new();
        let a = make_order_pair(Side::Buy, 100, 1, "AAA-USD");
        let b = make_order_pair(Side::Sell, 200, 1, "BBB-USD");
        book.insert_order(a.clone());
        book.insert_order(b.clone());

        let all = book.iter_all();
        assert_eq!(all.len(), 2);
        let ids: std::collections::HashSet<Uuid> = all.iter().map(|o| o.id).collect();
        assert!(ids.contains(&a.id));
        assert!(ids.contains(&b.id));
    }
}
