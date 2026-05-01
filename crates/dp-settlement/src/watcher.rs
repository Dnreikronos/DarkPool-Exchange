use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use alloy_primitives::{Address, FixedBytes};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::abi::BatchSettled;
use crate::helpers::bytes32_to_uuid;
use crate::SettlementError;

pub trait BatchSink: Send + Sync {
    fn on_batch_settled<'a>(
        &'a self,
        batch_id: Uuid,
        block_number: u64,
        tx_hash: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), SettlementError>> + Send + 'a>>;
}

pub struct Watcher<P, S> {
    provider: P,
    sink: S,
    contract: Address,
    cancel: CancellationToken,
}

impl<P: Provider + Send + Sync + 'static, S: BatchSink> Watcher<P, S> {
    pub fn new(provider: P, contract: Address, sink: S, cancel: CancellationToken) -> Self {
        Self {
            provider,
            sink,
            contract,
            cancel,
        }
    }

    /// Requires a provider that supports `eth_subscribe` with `logs` filter.
    pub async fn run(&self) {
        const MIN_BACKOFF: Duration = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(30);
        const CONSECUTIVE_FAIL_THRESHOLD: u32 = 5;
        let mut backoff = MIN_BACKOFF;
        let mut consecutive_failures: u32 = 0;

        loop {
            match self.run_once().await {
                Ok(_) => {
                    if self.cancel.is_cancelled() {
                        return;
                    }
                    backoff = MIN_BACKOFF;
                    consecutive_failures = 0;
                }
                Err(e) => {
                    if self.cancel.is_cancelled() {
                        return;
                    }
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= CONSECUTIVE_FAIL_THRESHOLD {
                        tracing::error!(
                            consecutive_failures,
                            "settlement watcher degraded: {e}"
                        );
                    } else {
                        tracing::warn!("settlement watcher: {e} (reconnect in {backoff:?})");
                    }
                }
            }

            tokio::select! {
                _ = self.cancel.cancelled() => return,
                _ = tokio::time::sleep(backoff) => {},
            }

            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    async fn run_once(&self) -> Result<(), SettlementError> {
        let filter = Filter::new()
            .address(self.contract)
            .event_signature(BatchSettled::SIGNATURE_HASH);

        let sub = self
            .provider
            .subscribe_logs(&filter)
            .await
            .map_err(|e| SettlementError::Rpc(e.to_string()))?;

        let mut stream = sub.into_stream();

        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => return Ok(()),
                item = stream.next() => {
                    let Some(log) = item else {
                        return Err(SettlementError::Rpc("subscription closed".into()));
                    };

                    if log.topics().len() < 2 {
                        tracing::warn!("BatchSettled log missing indexed batchId");
                        continue;
                    }

                    let batch_id_bytes: FixedBytes<32> = log.topics()[1];
                    let batch_id = match bytes32_to_uuid(batch_id_bytes) {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::warn!("decode batchId: {e}");
                            continue;
                        }
                    };

                    let block_number = log.block_number.unwrap_or(0);
                    let tx_hash = log
                        .transaction_hash
                        .map(|h| format!("{h:#x}"))
                        .unwrap_or_default();

                    if let Err(e) = self
                        .sink
                        .on_batch_settled(batch_id, block_number, tx_hash)
                        .await
                    {
                        tracing::warn!("sink error: {e}");
                    }
                }
            }
        }
    }
}
