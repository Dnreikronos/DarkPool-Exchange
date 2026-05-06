use dp_aggregator::AggregatorError;
use dp_crypto::CryptoError;
use dp_event::EventError;
use dp_settlement::SettlementError;
use dp_types::DarkPoolError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Validation(#[from] DarkPoolError),

    #[error("decrypt order: {0}")]
    Decrypt(#[source] CryptoError),

    #[error("aggregate batch: {0}")]
    Aggregate(#[source] AggregatorError),

    #[error("submit batch: {0}")]
    Submit(#[source] SettlementError),

    #[error(transparent)]
    Event(#[from] EventError),

    #[error("recover: decrypt order {order_id}: {source}")]
    RecoverDecrypt {
        order_id: uuid::Uuid,
        #[source]
        source: CryptoError,
    },

    #[error("recover: commitment mismatch for order {order_id}")]
    RecoverCommitmentMismatch { order_id: uuid::Uuid },

    #[error("recover: re-aggregate orphan auction {auction_id}: {source}")]
    RecoverReAggregate {
        auction_id: uuid::Uuid,
        #[source]
        source: AggregatorError,
    },

    #[error("build witness: missing in-memory secret for order {order_id}")]
    WitnessSecretMissing { order_id: uuid::Uuid },

    #[error("build witness: order {order_id} no longer in book")]
    WitnessOrderMissing { order_id: uuid::Uuid },

    #[error("commitment encoding: {0}")]
    CommitmentEncoding(String),

    #[error("pair not configured for settlement: {pair}")]
    PairNotConfigured { pair: String },
}
