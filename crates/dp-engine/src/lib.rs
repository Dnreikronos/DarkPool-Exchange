mod batch;
mod engine;
mod error;
mod recover;
mod state;
mod subscribe;
mod tick;

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

pub use engine::Engine;
pub use error::EngineError;
pub use state::{PairConfig, PendingBatch};
pub use subscribe::AuctionNotification;

pub const DEFAULT_AUCTION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
pub const DEFAULT_ORDER_TTL: std::time::Duration = std::time::Duration::from_secs(600);
pub const DEFAULT_SUBMIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const DEFAULT_MIN_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);
pub const DEFAULT_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(60);
pub const DEFAULT_SUBSCRIBER_CAPACITY: usize = 64;
