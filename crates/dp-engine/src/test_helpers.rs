use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ark_bn254::Fr;
use ark_ff::Zero;
use dp_aggregator::{AggregatorError, ProofAggregator};
use dp_auction::Match;
use dp_crypto::{CryptoError, DecryptedOrder, Decrypter};
use dp_event::{EventData, Store};
use dp_settlement::{SettlementError, SubmitBatchParams, SubmitSessionParams, Submitter};
use dp_types::EventType;
use dp_zk::witness::BatchWitness;
use parking_lot::Mutex;
use tokio::sync::Notify;
use uuid::Uuid;

// --- Submitters ---

pub struct StubSubmitter {
    pub fail: AtomicBool,
    pub calls: AtomicU32,
    pub last_proof: Mutex<Vec<u8>>,
    pub tx_hash: String,
}

impl StubSubmitter {
    pub fn new() -> Self {
        Self {
            fail: AtomicBool::new(false),
            calls: AtomicU32::new(0),
            last_proof: Mutex::new(Vec::new()),
            tx_hash: "0xstub".to_string(),
        }
    }

    pub fn set_fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Submitter for StubSubmitter {
    fn submit<'a>(
        &'a self,
        params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_proof.lock() = params.proof.clone();
            if self.fail.load(Ordering::SeqCst) {
                return Err(SettlementError::Rpc("stub forced failure".into()));
            }
            Ok(self.tx_hash.clone())
        })
    }

    fn submit_session<'a>(
        &'a self,
        params: &'a SubmitSessionParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_proof.lock() = params.proof.clone();
            if self.fail.load(Ordering::SeqCst) {
                return Err(SettlementError::Rpc("stub forced failure".into()));
            }
            Ok(self.tx_hash.clone())
        })
    }
}

// --- Aggregators ---

pub struct StubAggregator {
    pub proof: Vec<u8>,
    pub calls: AtomicU32,
}

impl StubAggregator {
    pub fn new(proof: Vec<u8>) -> Self {
        Self {
            proof,
            calls: AtomicU32::new(0),
        }
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ProofAggregator for StubAggregator {
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.proof.clone())
        })
    }

    fn fold_step<'a>(
        &'a self,
        _pair: String,
        _external_inputs: dp_zk::step_circuit::AuctionExternalInputs,
    ) -> Pin<Box<dyn Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn finalize<'a>(
        &'a self,
        _pair: String,
    ) -> Pin<
        Box<dyn Future<Output = Result<dp_zk::folding::FinalProof, AggregatorError>> + Send + 'a>,
    > {
        let proof_bytes = self.proof.clone();
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(dp_zk::folding::FinalProof {
                proof_bytes,
                z_0: [Fr::zero(); 3],
                z_n: [Fr::zero(); 3],
                n_steps: 1,
                policy_hash: Fr::zero(),
            })
        })
    }
}

pub struct FailingAggregator;

impl ProofAggregator for FailingAggregator {
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            Err(AggregatorError::ProcessFailed {
                exit_code: 1,
                stderr: "forced failure".into(),
            })
        })
    }
}

pub struct BlockingAggregator {
    pub release: Notify,
    pub started: Notify,
    pub calls: AtomicU32,
}

impl BlockingAggregator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            release: Notify::new(),
            started: Notify::new(),
            calls: AtomicU32::new(0),
        })
    }

    pub fn release(&self) {
        self.release.notify_waiters();
    }

    pub async fn wait_started(&self) {
        self.started.notified().await;
    }
}

impl ProofAggregator for BlockingAggregator {
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_waiters();
            self.release.notified().await;
            Ok(vec![0u8; 32])
        })
    }

    fn fold_step<'a>(
        &'a self,
        _pair: String,
        _external_inputs: dp_zk::step_circuit::AuctionExternalInputs,
    ) -> Pin<Box<dyn Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_waiters();
            self.release.notified().await;
            Ok(())
        })
    }

    fn finalize<'a>(
        &'a self,
        _pair: String,
    ) -> Pin<
        Box<dyn Future<Output = Result<dp_zk::folding::FinalProof, AggregatorError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Ok(dp_zk::folding::FinalProof {
                proof_bytes: vec![0u8; 32],
                z_0: [Fr::zero(); 3],
                z_n: [Fr::zero(); 3],
                n_steps: 1,
                policy_hash: Fr::zero(),
            })
        })
    }
}

// --- Decrypters ---

/// XOR-obfuscates plaintext (NOT cryptographically secure). Used as a privacy
/// canary: ensures plaintext bytes do not appear in serialized event log.
pub struct XorDecrypter {
    pub key: Vec<u8>,
}

impl XorDecrypter {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        plaintext
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect()
    }
}

impl Decrypter for XorDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>> {
        Box::pin(async move {
            let plain: Vec<u8> = ciphertext
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ self.key[i % self.key.len()])
                .collect();
            let order: DecryptedOrder = serde_json::from_slice(&plain)?;
            Ok(order)
        })
    }
}

// --- Helpers ---

pub fn count_events(store: &dyn Store, ty: EventType) -> usize {
    let mut count = 0;
    let mut after = 0u64;
    loop {
        let events = store.read_from(after, 1024).expect("read_from");
        if events.is_empty() {
            break;
        }
        after = events.last().unwrap().seq;
        for ev in events {
            if ev.event_type == ty {
                count += 1;
            }
        }
    }
    count
}

pub fn last_proof_for_batch(store: &dyn Store, batch_id: Uuid) -> Option<Vec<u8>> {
    let mut after = 0u64;
    let mut found = None;
    loop {
        let events = store.read_from(after, 1024).expect("read_from");
        if events.is_empty() {
            break;
        }
        after = events.last().unwrap().seq;
        for ev in events {
            if let EventData::BatchSubmitted {
                batch_id: bid,
                proof,
                ..
            } = &ev.data
            {
                if *bid == batch_id {
                    found = Some(proof.clone());
                }
            }
        }
    }
    found
}

pub async fn place_plaintext_order(
    engine: &crate::Engine,
    pair: &str,
    side: dp_types::Side,
    price: rust_decimal::Decimal,
    size: rust_decimal::Decimal,
    commitment_key: &str,
    ttl: Duration,
) -> Result<dp_types::Order, crate::EngineError> {
    let trader = alloy_primitives::Address::ZERO;
    let (commit, ct) = crate::engine::build_decrypted_ciphertext(
        trader,
        pair,
        side,
        price,
        size,
        commitment_key,
        ttl,
    );
    engine.place_encrypted_order(commit, vec![], ct, None).await
}
