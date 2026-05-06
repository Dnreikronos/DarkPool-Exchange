use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dp_event::{Event, EventData, EventError, Store};
use dp_types::{Fill, Order, Side};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use uuid::Uuid;

struct Inner {
    bids: HashMap<Uuid, Order>,
    asks: HashMap<Uuid, Order>,
    seq: u64,
}

pub struct OrderBook {
    inner: RwLock<Inner>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                bids: HashMap::new(),
                asks: HashMap::new(),
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

    pub fn apply(&self, event: &Event) {
        self.inner.write().apply(event);
    }

    pub fn insert_order(&self, order: Order) {
        let mut inner = self.inner.write();
        match order.side {
            Side::Buy => inner.bids.insert(order.id, order),
            Side::Sell => inner.asks.insert(order.id, order),
        };
    }

    pub fn bids(&self) -> Vec<Order> {
        let inner = self.inner.read();
        let mut out: Vec<Order> = inner.bids.values().cloned().collect();
        out.sort_by(|a, b| {
            b.price.cmp(&a.price).then(a.submitted_at.cmp(&b.submitted_at))
        });
        out
    }

    pub fn asks(&self) -> Vec<Order> {
        let inner = self.inner.read();
        let mut out: Vec<Order> = inner.asks.values().cloned().collect();
        out.sort_by(|a, b| {
            a.price.cmp(&b.price).then(a.submitted_at.cmp(&b.submitted_at))
        });
        out
    }

    pub fn collect_expired(&self, now: DateTime<Utc>) -> Vec<Event> {
        let inner = self.inner.read();
        let default_time = DateTime::<Utc>::default();
        let mut expired = Vec::new();

        for order in inner.bids.values().chain(inner.asks.values()) {
            if order.expires_at != default_time && order.expires_at <= now {
                expired.push(Event {
                    seq: 0,
                    event_type: dp_types::EventType::OrderExpired,
                    timestamp: DateTime::default(),
                    data: EventData::OrderExpired { order_id: order.id },
                });
            }
        }
        expired
    }

    pub fn find_order(&self, id: Uuid) -> Option<Order> {
        let inner = self.inner.read();
        inner
            .bids
            .get(&id)
            .or_else(|| inner.asks.get(&id))
            .cloned()
    }

    pub fn has_order(&self, id: Uuid) -> bool {
        let inner = self.inner.read();
        inner.bids.contains_key(&id) || inner.asks.contains_key(&id)
    }

    pub fn active_order_count(&self) -> usize {
        let inner = self.inner.read();
        inner.bids.len() + inner.asks.len()
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
                self.bids.remove(order_id);
                self.asks.remove(order_id);
            }
            EventData::OrderMatched { bid, ask, .. } => {
                self.apply_fill(bid);
                self.apply_fill(ask);
            }
            EventData::AuctionExecuted { .. }
            | EventData::BatchSubmitted { .. }
            | EventData::BatchConfirmed { .. }
            | EventData::BatchSettled { .. } => {}
        }
    }

    fn apply_fill(&mut self, fill: &Fill) {
        if let Some(order) = self.bids.get_mut(&fill.order_id) {
            order.remaining_size -= fill.size;
            if order.remaining_size <= Decimal::ZERO {
                self.bids.remove(&fill.order_id);
            }
            return;
        }
        if let Some(order) = self.asks.get_mut(&fill.order_id) {
            order.remaining_size -= fill.size;
            if order.remaining_size <= Decimal::ZERO {
                self.asks.remove(&fill.order_id);
            }
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
        Order {
            id: Uuid::new_v4(),
            trader: alloy_primitives::Address::ZERO,
            pair: "BTC-USD".into(),
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

        let bids = book.bids();
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

        let asks = book.asks();
        assert_eq!(asks[0].id, o2.id); // price 100, earlier
        assert_eq!(asks[1].id, o3.id); // price 100, later
        assert_eq!(asks[2].id, o1.id); // price 300
    }
}
