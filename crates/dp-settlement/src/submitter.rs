use std::future::Future;
use std::pin::Pin;

use dp_auction::Match;
use uuid::Uuid;

use crate::SettlementError;

pub trait Submitter: Send + Sync {
    fn submit<'a>(
        &'a self,
        batch_id: Uuid,
        auction_id: Uuid,
        matches: &'a [Match],
        proof: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>>;
}

pub struct NoopSubmitter;

impl Submitter for NoopSubmitter {
    fn submit<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _proof: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async {
            Ok("0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string())
        })
    }
}
