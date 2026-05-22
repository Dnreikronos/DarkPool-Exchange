//! IVC folding API using sonobe HyperNova.

use ark_bn254::{Fr, G1Projective as Projective};
use ark_grumpkin::Projective as Projective2;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use ark_std::rand::{CryptoRng, RngCore};
use folding_schemes::{
    commitment::pedersen::Pedersen,
    folding::{hypernova::HyperNova, nova::PreprocessorParam as HnPreprocessorParam},
    transcript::poseidon::poseidon_canonical_config,
    FoldingScheme,
};

use crate::step_circuit::{AuctionExternalInputs, AuctionStepCircuit};
use crate::ZkError;

// --- Type aliases ---------------------------------------------------------

pub type HN = HyperNova<
    Projective,
    Projective2,
    AuctionStepCircuit,
    Pedersen<Projective>,
    Pedersen<Projective2>,
    1,
    1,
    false,
>;
pub type HnPP = <HN as FoldingScheme<Projective, Projective2, AuctionStepCircuit>>::ProverParam;
pub type HnVP = <HN as FoldingScheme<Projective, Projective2, AuctionStepCircuit>>::VerifierParam;
type HnPrep =
    <HN as FoldingScheme<Projective, Projective2, AuctionStepCircuit>>::PreprocessorParam;
pub type HnIVCProof =
    <HN as FoldingScheme<Projective, Projective2, AuctionStepCircuit>>::IVCProof;

// --- Public types ---------------------------------------------------------

/// Preprocessed params for HyperNova (prover + verifier).
pub struct HyperNovaPublicParams {
    pub prover: HnPP,
    pub verifier: HnVP,
}

/// Mutable accumulator holding the live HyperNova IVC state.
pub struct FoldingAccumulator {
    pub(crate) hn: HN,
    pub z_0: [Fr; 3],
    pub n_steps: u64,
}

/// Serialized finalized proof — ready for on-chain submission.
#[derive(Clone, Debug)]
pub struct FinalProof {
    /// Serialized `HN::IVCProof` bytes.
    pub proof_bytes: Vec<u8>,
    pub z_0: [Fr; 3],
    pub z_n: [Fr; 3],
    pub n_steps: u64,
    /// Convenience copy of `z_n[2]` (policy_hash, invariant across rounds).
    pub policy_hash: Fr,
}

// --- Core functions -------------------------------------------------------

/// Generate HyperNova prover and verifier params for a given batch size.
pub fn generate_params<R: RngCore + CryptoRng>(
    batch_size: usize,
    rng: &mut R,
) -> Result<HyperNovaPublicParams, ZkError> {
    let circuit = AuctionStepCircuit { batch_size };
    let poseidon_cfg = poseidon_canonical_config::<Fr>();
    let prep: HnPrep = HnPreprocessorParam::new(poseidon_cfg, circuit);
    let (prover, verifier) =
        HN::preprocess(rng, &prep).map_err(|e| ZkError::Ivc(format!("preprocess: {e}")))?;
    Ok(HyperNovaPublicParams { prover, verifier })
}

/// Initialize a fresh IVC accumulator.
pub fn init_accumulator(
    params: &HyperNovaPublicParams,
    batch_size: usize,
    z_0: [Fr; 3],
) -> Result<FoldingAccumulator, ZkError> {
    let circuit = AuctionStepCircuit { batch_size };
    let params_tuple = (params.prover.clone(), params.verifier.clone());
    let hn = HN::init(&params_tuple, circuit, z_0.to_vec())
        .map_err(|e| ZkError::Ivc(format!("init: {e}")))?;
    Ok(FoldingAccumulator { hn, z_0, n_steps: 0 })
}

/// Fold one auction round into the accumulator.
pub fn fold_step<R: RngCore + CryptoRng>(
    acc: &mut FoldingAccumulator,
    external_inputs: AuctionExternalInputs,
    rng: &mut R,
) -> Result<(), ZkError> {
    acc.hn
        .prove_step(rng, external_inputs, None)
        .map_err(|e| ZkError::Ivc(format!("prove_step: {e}")))?;
    acc.n_steps += 1;
    Ok(())
}

/// Finalize the IVC chain into a serialized proof.
pub fn compress_and_finalize(acc: &FoldingAccumulator) -> Result<FinalProof, ZkError> {
    let ivc_proof = acc.hn.ivc_proof();
    let z_n: [Fr; 3] = ivc_proof
        .z_i
        .as_slice()
        .try_into()
        .map_err(|_| ZkError::Ivc(format!("z_i has wrong length (got {})", ivc_proof.z_i.len())))?;
    let policy_hash = z_n[2];

    let mut proof_bytes = Vec::new();
    ivc_proof
        .serialize_with_mode(&mut proof_bytes, Compress::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;

    Ok(FinalProof { proof_bytes, z_0: acc.z_0, z_n, n_steps: acc.n_steps, policy_hash })
}

/// Verify a finalized proof.
pub fn verify_final(
    params: &HyperNovaPublicParams,
    proof: &FinalProof,
) -> Result<(), ZkError> {
    let ivc_proof = HnIVCProof::deserialize_with_mode(
        proof.proof_bytes.as_slice(),
        Compress::Yes,
        Validate::Yes,
    )
    .map_err(|e| ZkError::Serialize(format!("deserialize ivc_proof: {e}")))?;

    HN::verify(params.verifier.clone(), ivc_proof)
        .map_err(|e| ZkError::Ivc(format!("verify: {e}")))?;
    Ok(())
}
