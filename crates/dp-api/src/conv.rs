use dp_engine::AuctionNotification;
use dp_types::{Order, Side};
use tonic::Status;
use uuid::Uuid;

use crate::pb;

pub fn side_to_proto(side: Side) -> i32 {
    match side {
        Side::Buy => pb::Side::Buy as i32,
        Side::Sell => pb::Side::Sell as i32,
    }
}

pub fn order_to_proto(o: &Order) -> pb::OrderInfo {
    pb::OrderInfo {
        id: o.id.to_string(),
        pair: o.pair.clone(),
        side: side_to_proto(o.side),
        price: o.price.to_string(),
        size: o.size.to_string(),
        remaining_size: o.remaining_size.to_string(),
        commitment_key: o.commitment_key.clone(),
        submitted_at_unix: o.submitted_at.timestamp(),
        expires_at_unix: o.expires_at.timestamp(),
    }
}

pub fn notification_to_event(n: &AuctionNotification) -> pb::AuctionEvent {
    pb::AuctionEvent {
        auction_id: n.auction_id.to_string(),
        pair: n.pair.clone(),
        clearing_price: n.clearing_price.to_string(),
        matched_volume: n.matched_volume.to_string(),
        match_count: n.match_count as i32,
        timestamp_unix: n.timestamp.timestamp(),
    }
}

pub fn notification_to_summary(n: &AuctionNotification) -> pb::AuctionSummary {
    pb::AuctionSummary {
        auction_id: n.auction_id.to_string(),
        pair: n.pair.clone(),
        clearing_price: n.clearing_price.to_string(),
        matched_volume: n.matched_volume.to_string(),
        match_count: n.match_count as i32,
        timestamp_unix: n.timestamp.timestamp(),
    }
}

pub fn aggregate_levels(orders: Vec<Order>) -> Vec<pb::PriceLevel> {
    use std::collections::HashMap;
    let mut agg: HashMap<String, (rust_decimal::Decimal, i32)> = HashMap::new();
    let mut order_seen: Vec<String> = Vec::new();
    for o in orders {
        let key = o.price.to_string();
        let entry = agg.entry(key.clone()).or_insert_with(|| {
            order_seen.push(key.clone());
            (rust_decimal::Decimal::ZERO, 0)
        });
        entry.0 += o.remaining_size;
        entry.1 += 1;
    }
    order_seen
        .into_iter()
        .map(|k| {
            let (total, count) = agg.remove(&k).unwrap();
            pb::PriceLevel {
                price: k,
                total_size: total.to_string(),
                order_count: count,
            }
        })
        .collect()
}

pub fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(s).map_err(|e| Status::invalid_argument(format!("invalid order_id: {}", e)))
}
