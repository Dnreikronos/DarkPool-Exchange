mod aggregator;
mod subprocess;
mod inline;

pub use aggregator::{NoopAggregator, ProofAggregator};
pub use subprocess::SubprocessAggregator;
pub use inline::InlineFoldingAggregator;

use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AggregatorError {
    #[error("aggregator timed out")]
    Timeout,
    #[error("aggregator exited {exit_code}: {stderr}")]
    ProcessFailed { exit_code: i32, stderr: String },
    #[error("aggregator binary not found: {0}")]
    BinaryNotFound(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error("zk error: {0}")]
    Zk(String),
}
