use std::future::Future;
use std::pin::Pin;

use crate::{SettlementError, SubmitBatchParams, SubmitSessionParams};

pub trait Submitter: Send + Sync {
    fn submit<'a>(
        &'a self,
        params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>>;

    /// Submit an IVC-proved session and settle its auction in two sequential
    /// on-chain calls: `submitSession(...)` then `settleAuction(...)`.
    /// Returns the settleAuction tx hash on success.
    ///
    /// Default impl returns `SettlementError::Rpc` so submitters that don't
    /// support the IVC path surface a loud error rather than silently
    /// no-op'ing — mirrors the migration pattern used in
    /// `ProofAggregator::fold_step`.
    fn submit_session<'a>(
        &'a self,
        _params: &'a SubmitSessionParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            Err(SettlementError::Rpc(
                "submit_session not supported by this submitter".into(),
            ))
        })
    }
}

pub struct NoopSubmitter;

impl Submitter for NoopSubmitter {
    fn submit<'a>(
        &'a self,
        _params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async {
            Ok("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
        })
    }

    fn submit_session<'a>(
        &'a self,
        _params: &'a SubmitSessionParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async {
            Ok("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
        })
    }
}
