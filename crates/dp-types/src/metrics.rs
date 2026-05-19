//! Metric-name constants shared between the engine (which emits) and the
//! API observability layer (which registers descriptions + force-registers
//! zero values). Lives here so both crates link against the same strings —
//! a typo in either side would otherwise silently fork the series.

pub const M_AUCTIONS_TOTAL: &str = "darkpool_auctions_total";
pub const M_AUCTION_DURATION: &str = "darkpool_auction_duration_seconds";
pub const M_ORDERS_PLACED: &str = "darkpool_orders_placed_total";
pub const M_ORDERS_MATCHED: &str = "darkpool_orders_matched_total";
pub const M_ORDERS_EXPIRED: &str = "darkpool_orders_expired_total";
pub const M_CLEARING_PRICE: &str = "darkpool_clearing_price";
pub const M_BATCH_SUBMISSION_DURATION: &str = "darkpool_batch_submission_duration_seconds";
pub const M_SETTLEMENT_CONFIRMATIONS: &str = "darkpool_settlement_confirmations_total";
pub const M_ACTIVE_ORDERS: &str = "darkpool_active_orders";
pub const M_EVENT_LOG_SIZE_BYTES: &str = "darkpool_event_log_size_bytes";
