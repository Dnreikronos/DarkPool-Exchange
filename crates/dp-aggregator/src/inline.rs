//! `InlineFoldingAggregator`: folds auction rounds directly in-process using HyperNova.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

use dp_zk::folding::{
    compress_and_finalize, fold_step as zk_fold_step, init_accumulator, FinalProof,
    FoldingAccumulator, HyperNovaPublicParams,
};
use dp_zk::pedersen::poseidon_config;
use dp_zk::step_circuit::AuctionExternalInputs;

use crate::aggregator::ProofAggregator;
use crate::AggregatorError;

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::{poseidon::PoseidonSponge, CryptographicSponge};
use ark_ff::Zero;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use dp_auction::Match;
use dp_zk::witness::BatchWitness;

struct IvcRoundState {
    acc: FoldingAccumulator,
    /// Number of rounds folded so far (informational; not consumed by this struct).
    #[allow(dead_code)]
    round_index: u64,
}

/// In-process HyperNova IVC aggregator.
///
/// Each trading pair (`"ETH/USDC"`, etc.) gets its own independent IVC chain.
/// Call [`fold_step`][ProofAggregator::fold_step] once per auction round, then
/// [`finalize`][ProofAggregator::finalize] to obtain the serialized proof ready
/// for on-chain submission.
///
/// The `aggregate` method is a no-op for the IVC path — proof accumulation
/// happens through `fold_step`/`finalize` instead.
pub struct InlineFoldingAggregator {
    params: Arc<HyperNovaPublicParams>,
    state: Arc<Mutex<HashMap<String, IvcRoundState>>>,
    batch_size: usize,
    /// How many rounds to fold before `finalize` is expected.  Currently
    /// informational; callers decide when to finalize.
    #[allow(dead_code)]
    finalize_every: u64,
}

impl InlineFoldingAggregator {
    /// Create a new aggregator.
    ///
    /// * `params` — pre-generated HyperNova public params (see
    ///   [`dp_zk::folding::generate_params`]).
    /// * `batch_size` — circuit batch size; must match the `batch_size` used
    ///   when generating `params`.
    /// * `finalize_every` — advisory cadence (in rounds) for finalization.
    pub fn new(params: Arc<HyperNovaPublicParams>, batch_size: usize, finalize_every: u64) -> Self {
        Self {
            params,
            state: Arc::new(Mutex::new(HashMap::new())),
            batch_size,
            finalize_every,
        }
    }
}

impl ProofAggregator for InlineFoldingAggregator {
    /// No-op for the IVC path — proof accumulation is done via
    /// [`fold_step`][ProofAggregator::fold_step] and
    /// [`finalize`][ProofAggregator::finalize].
    fn aggregate<'a>(
        &'a self,
        _batch_id: Uuid,
        _auction_id: Uuid,
        _matches: &'a [Match],
        _witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            Err(AggregatorError::Zk(
                "aggregate is not supported for InlineFoldingAggregator; use fold_step/finalize"
                    .into(),
            ))
        })
    }

    fn fold_step<'a>(
        &'a self,
        pair: String,
        external_inputs: AuctionExternalInputs,
    ) -> Pin<Box<dyn Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
        let params = Arc::clone(&self.params);
        let state = Arc::clone(&self.state);
        let batch_size = self.batch_size;

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let z_0 = compute_z_0(&external_inputs);
                let mut guard = state.lock();
                let entry = match guard.entry(pair) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let acc = init_accumulator(&params, batch_size, z_0)
                            .map_err(|e| AggregatorError::Zk(format!("init_accumulator: {e}")))?;
                        e.insert(IvcRoundState {
                            acc,
                            round_index: 0,
                        })
                    }
                };
                // TODO(production): seed StdRng from OsRng or a hardware source
                // rather than a fixed seed for production use.
                let mut rng = StdRng::from_entropy();
                zk_fold_step(&mut entry.acc, external_inputs, &mut rng)
                    .map_err(|e| AggregatorError::Zk(e.to_string()))?;
                entry.round_index += 1;
                Ok(())
            })
            .await
            .map_err(|e| AggregatorError::Zk(format!("spawn_blocking: {e}")))?
        })
    }

    fn finalize<'a>(
        &'a self,
        pair: String,
    ) -> Pin<Box<dyn Future<Output = Result<FinalProof, AggregatorError>> + Send + 'a>> {
        let state = Arc::clone(&self.state);

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let guard = state.lock();
                let entry = guard
                    .get(&pair)
                    .ok_or_else(|| AggregatorError::Zk(format!("no IVC state for pair {pair}")))?;
                compress_and_finalize(&entry.acc).map_err(|e| AggregatorError::Zk(e.to_string()))
            })
            .await
            .map_err(|e| AggregatorError::Zk(format!("spawn_blocking: {e}")))?
        })
    }
}

/// Compute z_0 = [0, 0, poseidon(min_size, min_price, position_limit), 0, 0]
/// from the external inputs of the first round. This matches the `initial_z`
/// helper in `dp-zk`'s step circuit tests. The two trailing slots are the empty
/// settlement hash-chain accumulator (#153) and the empty admitted-set chain
/// (#157).
fn compute_z_0(ext: &AuctionExternalInputs) -> [Fr; 5] {
    let cfg = poseidon_config();
    let mut s = PoseidonSponge::<Fr>::new(&cfg);
    s.absorb(&vec![ext.min_size, ext.min_price, ext.position_limit]);
    let policy_hash = s.squeeze_field_elements::<Fr>(1)[0];
    [Fr::zero(), Fr::zero(), policy_hash, Fr::zero(), Fr::zero()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `InlineFoldingAggregator::new` constructs successfully and
    /// that the type implements `ProofAggregator + Send + Sync`.
    ///
    /// Generating real HyperNova params takes ~80 s, so the IVC fold/finalize
    /// path is covered by the `#[ignore]`d test below and by the integration
    /// tests in `dp-zk/tests/ivc.rs`.
    #[test]
    fn aggregator_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InlineFoldingAggregator>();
    }

    /// Full fold → finalize round-trip.  Ignored by default because
    /// `generate_params` takes ~80 s in CI.  Run locally with:
    ///
    /// ```sh
    /// cargo test -p dp-aggregator --features hypernova -- --ignored fold_initializes_state_for_pair
    /// ```
    #[tokio::test]
    #[ignore]
    async fn fold_initializes_state_for_pair() {
        use dp_zk::folding::generate_params;

        let mut rng = StdRng::seed_from_u64(42);
        let batch_size = 1usize;
        let params = Arc::new(generate_params(batch_size, &mut rng).expect("generate_params"));
        let agg = InlineFoldingAggregator::new(params, batch_size, 10);

        let ext = AuctionExternalInputs::default();
        agg.fold_step("ETH/USDC".into(), ext)
            .await
            .expect("fold_step");

        let proof = agg.finalize("ETH/USDC".into()).await.expect("finalize");
        assert_eq!(proof.n_steps, 1, "expected exactly one step");
        assert_eq!(proof.z_0[0], Fr::zero());
        assert_eq!(proof.z_0[1], Fr::zero());
    }
}
