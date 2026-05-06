use std::future::Future;
use std::pin::Pin;

use crate::{SettlementError, SubmitBatchParams};

pub trait Submitter: Send + Sync {
    fn submit<'a>(
        &'a self,
        params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>>;
}

pub struct NoopSubmitter;

impl Submitter for NoopSubmitter {
    fn submit<'a>(
        &'a self,
        _params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async {
            Ok("0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string())
        })
    }
}
