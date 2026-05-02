use std::future::Future;
use std::pin::Pin;

use dp_auction::Match;
use dp_zk::witness::BatchWitness;
use uuid::Uuid;

use crate::AggregatorError;

pub trait ProofAggregator: Send + Sync {
    fn aggregate<'a>(
        &'a self,
        batch_id: Uuid,
        auction_id: Uuid,
        matches: &'a [Match],
        witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>>;
}

pub struct NoopAggregator;

impl ProofAggregator for NoopAggregator {
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async { Ok(vec![0u8; 32]) })
    }
}
