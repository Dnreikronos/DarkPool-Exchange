use dp_engine::EngineError;
use dp_types::DarkPoolError;
use tonic::Status;

pub fn engine_error_to_status(err: EngineError) -> Status {
    match err {
        EngineError::Validation(DarkPoolError::OrderNotFound) => {
            Status::not_found(DarkPoolError::OrderNotFound.to_string())
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
