use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use alloy_primitives::U256;
use chrono::Utc;
use dp_event::{Event, EventData};
use dp_settlement::{BatchSink, SettlementError, SettlementMatch, SubmitBatchParams};
use dp_types::EventType;
use uuid::Uuid;

use crate::engine::Engine;
use crate::error::EngineError;
use crate::state::{compute_backoff, PendingBatch};
use crate::tick::PendingAggregation;

impl Engine {
    pub(crate) fn finalize_pending_batch(
        &self,
        p: &PendingAggregation,
        proof: Vec<u8>,
        public_inputs: [U256; 6],
        settlement_matches: Vec<SettlementMatch>,
    ) -> Result<(), EngineError> {
        let mut events = [Event {
            seq: 0,
            event_type: EventType::BatchSubmitted,
            timestamp: p.auction_at,
            data: EventData::BatchSubmitted {
                batch_id: p.batch_id,
                auction_id: p.auction_id,
                tx_hash: String::new(),
                match_count: p.matches.len() as u32,
                proof: proof.clone(),
            },
        }];

        let mut state = self.inner.state.lock();
        self.inner.store.append(&mut events)?;
        state.book.apply(&events[0]);
        state.pending_batches.insert(
            p.batch_id,
            PendingBatch {
                batch_id: p.batch_id,
                auction_id: p.auction_id,
                matches: p.matches.clone(),
                settlement_matches,
                proof,
                public_inputs,
                attempts: 0,
                next_attempt: None,
                submitting: false,
            },
        );
        Ok(())
    }

    pub(crate) async fn resubmit_pending_except(&self, skip: &HashSet<Uuid>) {
        let now = Instant::now();
        let ids: Vec<Uuid> = {
            let state = self.inner.state.lock();
            state
                .pending_batches
                .iter()
                .filter(|(id, pb)| {
                    !skip.contains(*id) && pb.next_attempt.map(|t| now >= t).unwrap_or(true)
                })
                .map(|(id, _)| *id)
                .collect()
        };

        for id in ids {
            if let Err(e) = self.submit_batch(id).await {
                tracing::warn!(batch_id = %id, "retry submit: {e}");
            }
        }
    }

    pub async fn resubmit_pending(&self) {
        let ids: Vec<Uuid> = {
            let state = self.inner.state.lock();
            state.pending_batches.keys().copied().collect()
        };
        for id in ids {
            if let Err(e) = self.submit_batch(id).await {
                tracing::warn!(batch_id = %id, "resubmit: {e}");
            }
        }
    }

    pub(crate) async fn submit_batch(&self, batch_id: Uuid) -> Result<(), EngineError> {
        // Acquire submitting flag + snapshot inputs.
        let snapshot = {
            let mut state = self.inner.state.lock();
            let Some(pb) = state.pending_batches.get_mut(&batch_id) else {
                return Ok(());
            };
            if pb.submitting {
                return Ok(());
            }
            pb.submitting = true;
            let auction_id = pb.auction_id;
            let settlement_matches = pb.settlement_matches.clone();
            let proof = pb.proof.clone();
            let public_inputs = pb.public_inputs;
            let timeout = state.submit_timeout;
            (
                auction_id,
                settlement_matches,
                proof,
                public_inputs,
                timeout,
            )
        };

        let (auction_id, settlement_matches, proof, public_inputs, timeout) = snapshot;
        let submitter = self.inner.submitter.read().clone();

        let params = SubmitBatchParams {
            batch_id,
            auction_id,
            proof,
            public_inputs,
            matches: settlement_matches,
        };

        let result = tokio::time::timeout(timeout, submitter.submit(&params)).await;

        let submit_outcome: Result<String, SettlementError> = match result {
            Ok(r) => r,
            Err(_) => Err(SettlementError::Rpc("submit timeout".to_string())),
        };

        let tx_hash = match submit_outcome {
            Ok(h) => h,
            Err(e) => {
                self.note_submit_failure(batch_id);
                return Err(EngineError::Submit(e));
            }
        };

        // Persist BatchConfirmed off-lock.
        let mut events = [Event {
            seq: 0,
            event_type: EventType::BatchConfirmed,
            timestamp: Utc::now(),
            data: EventData::BatchConfirmed {
                batch_id,
                tx_hash: tx_hash.clone(),
            },
        }];
        if let Err(e) = self.inner.store.append(&mut events) {
            self.note_submit_failure(batch_id);
            return Err(EngineError::Event(e));
        }

        let mut state = self.inner.state.lock();
        state.book.apply(&events[0]);
        state.pending_batches.remove(&batch_id);
        Ok(())
    }

    pub(crate) fn note_submit_failure(&self, batch_id: Uuid) {
        let mut state = self.inner.state.lock();
        let min = state.min_backoff;
        let max = state.max_backoff;
        if let Some(pb) = state.pending_batches.get_mut(&batch_id) {
            pb.attempts = pb.attempts.saturating_add(1);
            pb.next_attempt = Some(Instant::now() + compute_backoff(min, max, pb.attempts));
            pb.submitting = false;
        }
    }
}

impl BatchSink for Engine {
    fn on_batch_settled<'a>(
        &'a self,
        batch_id: Uuid,
        block_number: u64,
        tx_hash: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            let mut events = [Event {
                seq: 0,
                event_type: EventType::BatchSettled,
                timestamp: Utc::now(),
                data: EventData::BatchSettled {
                    batch_id,
                    block_number,
                    tx_hash,
                },
            }];
            if let Err(e) = self.inner.store.append(&mut events) {
                tracing::warn!(batch_id = %batch_id, "persist BatchSettled: {e}");
                return Ok(());
            }
            let mut state = self.inner.state.lock();
            state.book.apply(&events[0]);
            state.pending_batches.remove(&batch_id);
            Ok(())
        })
    }
}
