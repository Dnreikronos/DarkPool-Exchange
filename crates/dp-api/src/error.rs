use dp_crypto::CryptoError;
use dp_engine::EngineError;
use dp_types::DarkPoolError;
use tonic::Status;

pub fn engine_error_to_status(err: EngineError) -> Status {
    match err {
        EngineError::Validation(DarkPoolError::OrderNotFound) => {
            Status::not_found(DarkPoolError::OrderNotFound.to_string())
        }
        EngineError::Validation(e @ DarkPoolError::PairNotRegistered(_)) => {
            Status::not_found(e.to_string())
        }
        EngineError::Validation(e @ DarkPoolError::PairAlreadyRegistered(_)) => {
            Status::already_exists(e.to_string())
        }
        EngineError::Validation(
            e @ (DarkPoolError::PairNotAccepting(_) | DarkPoolError::CannotSuspendDelistedPair(_)),
        ) => Status::failed_precondition(e.to_string()),
        EngineError::Validation(e @ DarkPoolError::TraderAddressMismatch { .. }) => {
            Status::permission_denied(e.to_string())
        }
        EngineError::Validation(e @ DarkPoolError::DuplicateOrder) => {
            Status::already_exists(e.to_string())
        }
        EngineError::Decrypt(CryptoError::DeserializationFailed(e)) => {
            Status::invalid_argument(e.to_string())
        }
        EngineError::Validation(
            e @ (DarkPoolError::PairRequired
            | DarkPoolError::PriceMustBePositive
            | DarkPoolError::SizeMustBePositive
            | DarkPoolError::CommitmentKeyRequired
            | DarkPoolError::LimitMustBePositive
            | DarkPoolError::OrderSizeBelowMinimum { .. }
            | DarkPoolError::OrderPriceNotOnTick { .. }
            | DarkPoolError::SaltInvalid
            | DarkPoolError::InvalidOrderProof
            | DarkPoolError::InvalidPair(_)),
        ) => Status::invalid_argument(e.to_string()),
        other => Status::internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Code;

    #[test]
    fn order_not_found_maps_to_not_found() {
        let s = engine_error_to_status(EngineError::Validation(DarkPoolError::OrderNotFound));
        assert_eq!(s.code(), Code::NotFound);
        assert_eq!(s.message(), DarkPoolError::OrderNotFound.to_string());
    }

    #[test]
    fn pair_not_registered_maps_to_not_found() {
        let s = engine_error_to_status(EngineError::Validation(DarkPoolError::PairNotRegistered(
            "ETH/USDC".into(),
        )));
        assert_eq!(s.code(), Code::NotFound);
        assert!(s.message().contains("ETH/USDC"));
    }

    #[test]
    fn pair_already_registered_maps_to_already_exists() {
        let s = engine_error_to_status(EngineError::Validation(
            DarkPoolError::PairAlreadyRegistered("ETH/USDC".into()),
        ));
        assert_eq!(s.code(), Code::AlreadyExists);
    }

    #[test]
    fn pair_not_accepting_maps_to_failed_precondition() {
        let s = engine_error_to_status(EngineError::Validation(DarkPoolError::PairNotAccepting(
            "ETH/USDC".into(),
        )));
        assert_eq!(s.code(), Code::FailedPrecondition);
    }

    #[test]
    fn validation_bag_maps_to_invalid_argument() {
        for err in [
            DarkPoolError::PairRequired,
            DarkPoolError::PriceMustBePositive,
            DarkPoolError::SizeMustBePositive,
            DarkPoolError::CommitmentKeyRequired,
            DarkPoolError::LimitMustBePositive,
            DarkPoolError::OrderSizeBelowMinimum {
                pair: "ETH/USDC".into(),
                min: "0.01".into(),
            },
            DarkPoolError::OrderPriceNotOnTick {
                pair: "ETH/USDC".into(),
                tick: "0.01".into(),
            },
            DarkPoolError::SaltInvalid,
            DarkPoolError::InvalidOrderProof,
            DarkPoolError::InvalidPair("bad".into()),
        ] {
            let s = engine_error_to_status(EngineError::Validation(err));
            assert_eq!(s.code(), Code::InvalidArgument);
        }
    }

    #[test]
    fn trader_address_mismatch_maps_to_permission_denied() {
        let s = engine_error_to_status(EngineError::Validation(
            DarkPoolError::TraderAddressMismatch {
                expected: "0x1111".into(),
                found: "***".into(),
            },
        ));
        assert_eq!(s.code(), Code::PermissionDenied);
    }

    #[test]
    fn duplicate_order_maps_to_already_exists() {
        let s = engine_error_to_status(EngineError::Validation(DarkPoolError::DuplicateOrder));
        assert_eq!(s.code(), Code::AlreadyExists);
    }

    #[test]
    fn non_validation_engine_error_maps_to_internal() {
        let s = engine_error_to_status(EngineError::WitnessOrderMissing {
            order_id: uuid::Uuid::nil(),
        });
        assert_eq!(s.code(), Code::Internal);
    }
}
