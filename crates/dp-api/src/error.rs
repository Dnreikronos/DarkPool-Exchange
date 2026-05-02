use dp_engine::EngineError;
use dp_types::DarkPoolError;
use tonic::Status;

use crate::validation::MSG_COMMITMENT_MISMATCH;

pub fn engine_error_to_status(err: EngineError) -> Status {
    match err {
        EngineError::Validation(DarkPoolError::OrderNotFound) => {
            Status::not_found(DarkPoolError::OrderNotFound.to_string())
        }
        EngineError::Validation(DarkPoolError::CommitmentMismatch) => {
            Status::invalid_argument(MSG_COMMITMENT_MISMATCH)
        }
        EngineError::Validation(
            e @ (DarkPoolError::PairRequired
            | DarkPoolError::PriceMustBePositive
            | DarkPoolError::SizeMustBePositive
            | DarkPoolError::CommitmentKeyRequired
            | DarkPoolError::LimitMustBePositive),
        ) => Status::invalid_argument(e.to_string()),
        other => Status::internal(other.to_string()),
    }
}
