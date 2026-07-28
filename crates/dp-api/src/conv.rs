use dp_engine::{AuctionNotification, PairConfig, PairStatus};
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

/// `pb::AuctionEvent` and `pb::AuctionSummary` are distinct proto
/// messages with identical fields. The macro keeps the field-by-field
/// projection in one place so adding a field doesn't require updating
/// two near-copies.
macro_rules! notification_proto {
    ($ty:path, $n:expr) => {{
        $ty {
            auction_id: $n.auction_id.to_string(),
            pair: $n.pair.clone(),
            clearing_price: $n.clearing_price.to_string(),
            matched_volume: $n.matched_volume.to_string(),
            match_count: $n.match_count as i32,
            timestamp_unix: $n.timestamp.timestamp(),
        }
    }};
}

pub fn notification_to_event(n: &AuctionNotification) -> pb::AuctionEvent {
    notification_proto!(pb::AuctionEvent, n)
}

pub fn notification_to_summary(n: &AuctionNotification) -> pb::AuctionSummary {
    notification_proto!(pb::AuctionSummary, n)
}

pub fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(s).map_err(|e| Status::invalid_argument(format!("invalid order_id: {}", e)))
}

pub fn pair_status_to_proto(s: PairStatus) -> i32 {
    match s {
        PairStatus::Active => pb::PairStatus::Active as i32,
        PairStatus::Suspended => pb::PairStatus::Suspended as i32,
        PairStatus::Delisted => pb::PairStatus::Delisted as i32,
    }
}

/// Saturate `Duration::as_millis()` (u128) into the proto's `u32` slot.
/// The on-disk event log uses `u64` ms, so an interval that fits the
/// event log can in principle exceed `u32::MAX` (~49.7 days) and would
/// otherwise wrap silently. Real intervals are 5s-class, so this is
/// defensive: we clamp + warn rather than fail the response.
fn auction_interval_to_proto_ms(d: std::time::Duration) -> u32 {
    let ms = d.as_millis();
    if ms > u32::MAX as u128 {
        tracing::warn!(
            millis = %ms,
            "auction_interval exceeds u32::MAX; clamping to u32::MAX in proto response"
        );
        u32::MAX
    } else {
        ms as u32
    }
}

pub fn pair_config_to_proto(pair: &str, cfg: &PairConfig) -> pb::PairInfo {
    pb::PairInfo {
        pair: pair.to_string(),
        base_token: format!("{:#x}", cfg.base_token),
        quote_token: format!("{:#x}", cfg.quote_token),
        min_order_size: cfg.min_order_size.to_string(),
        tick_size: cfg.tick_size.to_string(),
        auction_interval_ms: cfg.auction_interval.map(auction_interval_to_proto_ms),
        status: pair_status_to_proto(cfg.status),
        base_decimals: Some(u32::from(cfg.base_decimals)),
        quote_decimals: Some(u32::from(cfg.quote_decimals)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auction_interval_to_proto_ms_normal_value() {
        let d = std::time::Duration::from_millis(5_000);
        assert_eq!(auction_interval_to_proto_ms(d), 5_000);
    }

    #[test]
    fn auction_interval_to_proto_ms_saturates_above_u32_max() {
        // u32::MAX milliseconds + 1
        let d = std::time::Duration::from_millis(u32::MAX as u64 + 1);
        assert_eq!(auction_interval_to_proto_ms(d), u32::MAX);
    }
}
