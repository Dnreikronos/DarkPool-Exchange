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

    /// Fold one auction round into the IVC accumulator for `pair`.
    ///
    /// Implementors that do not support IVC may rely on the default, which
    /// returns `AggregatorError::Zk`.
    fn fold_step<'a>(
        &'a self,
        pair: String,
        external_inputs: dp_zk::step_circuit::AuctionExternalInputs,
    ) -> Pin<Box<dyn Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
        let _ = (pair, external_inputs);
        Box::pin(async move {
            Err(AggregatorError::Zk(
                "fold_step not supported by this aggregator".into(),
            ))
        })
    }

    /// Finalize the IVC chain for `pair` into a [`dp_zk::folding::FinalProof`].
    ///
    /// Implementors that do not support IVC may rely on the default, which
    /// returns `AggregatorError::Zk`.
    fn finalize<'a>(
        &'a self,
        pair: String,
    ) -> Pin<
        Box<dyn Future<Output = Result<dp_zk::folding::FinalProof, AggregatorError>> + Send + 'a>,
    > {
        let _ = pair;
        Box::pin(async move {
            Err(AggregatorError::Zk(
                "finalize not supported by this aggregator".into(),
            ))
        })
    }
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
        Box::pin(async move {
            use ark_bn254::Fr;
            use ark_ff::Zero;
            Ok(dp_zk::folding::FinalProof {
                proof_bytes: vec![0u8; 32],
                z_0: [Fr::zero(); 4],
                z_n: [Fr::zero(); 4],
                n_steps: 0,
                policy_hash: Fr::zero(),
            })
        })
    }
}
