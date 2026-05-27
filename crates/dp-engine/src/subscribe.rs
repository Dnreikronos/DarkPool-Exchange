use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AuctionNotification {
    pub auction_id: Uuid,
    pub pair: String,
    pub clearing_price: Decimal,
    pub matched_volume: Decimal,
    pub match_count: u32,
    pub timestamp: DateTime<Utc>,
}
