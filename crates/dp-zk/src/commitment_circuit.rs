//! Groth16 circuit proving knowledge of a Poseidon commitment preimage.
//!
//! Public input:  commitment (1 Fr element)
//! Private witness: (trader_id, side, limit_price, size, salt)
//! Constraint:    poseidon(trader_id, side, limit_price, size, salt) == commitment

use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::snark::SNARK;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::constraints::PoseidonSpongeVar;
use ark_groth16::Groth16;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use ark_std::rand::{CryptoRng, RngCore};

use crate::pedersen::{commit_native, poseidon_config, OrderCommitmentInput};

#[derive(Clone, Debug)]
pub struct CommitmentPreimageCircuit {
    pub trader_id: Fr,
    pub side: Fr,
    pub limit_price: Fr,
    pub size: Fr,
    pub salt: Fr,
}

impl ConstraintSynthesizer<Fr> for CommitmentPreimageCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let cfg = poseidon_config();

        let trader_id = FpVar::new_witness(cs.clone(), || Ok(self.trader_id))?;
        let side = FpVar::new_witness(cs.clone(), || Ok(self.side))?;
        let limit_price = FpVar::new_witness(cs.clone(), || Ok(self.limit_price))?;
        let size = FpVar::new_witness(cs.clone(), || Ok(self.size))?;
        let salt = FpVar::new_witness(cs.clone(), || Ok(self.salt))?;

        let expected = compute_commitment_native(&self);
        let expected_var = FpVar::new_input(cs.clone(), || Ok(expected))?;

        let mut sponge = PoseidonSpongeVar::<Fr>::new(cs, &cfg);
        sponge.absorb(&[trader_id, side, limit_price, size, salt].as_ref())?;
        let computed = sponge.squeeze_field_elements(1)?[0].clone();

        computed.enforce_equal(&expected_var)?;

        Ok(())
    }
}

fn compute_commitment_native(circuit: &CommitmentPreimageCircuit) -> Fr {
    let input = OrderCommitmentInput {
        trader_id: circuit.trader_id,
        side: circuit.side,
        limit_price: circuit.limit_price,
        size: circuit.size,
        salt: circuit.salt,
    };
    commit_native(&input)
}

pub struct SingleOrderProof {
    pub commitment: Fr,
    pub proof_bytes: Vec<u8>,
    pub vk_bytes: Vec<u8>,
}

type Groth16Keys = (
    ark_groth16::ProvingKey<Bn254>,
    ark_groth16::VerifyingKey<Bn254>,
);

/// Run the **one-time** trusted setup for `CommitmentPreimageCircuit` and
/// return the proving/verifying key pair.
///
/// This is the only sound way to obtain a verifying key: setup is run once,
/// the resulting VK is pinned as canonical, and every prover then uses the
/// matching proving key. The verifier must NEVER accept a prover-supplied
/// VK — see [`verify_proof_with_vk`].
///
/// The circuit shape is fixed (witness values don't affect the constraint
/// system here — they only affect the public-input value), so any concrete
/// circuit instance produces an interchangeable key pair for the same RNG.
pub fn generate_keys<R: RngCore + CryptoRng>(rng: &mut R) -> Result<Groth16Keys, crate::ZkError> {
    // The witness values are placeholders: setup only inspects the
    // constraint structure, which is identical for every instance of this
    // circuit. Using zeros keeps key generation independent of any order.
    let shape = CommitmentPreimageCircuit {
        trader_id: Fr::from(0u64),
        side: Fr::from(0u64),
        limit_price: Fr::from(0u64),
        size: Fr::from(0u64),
        salt: Fr::from(0u64),
    };
    Groth16::<Bn254>::circuit_specific_setup(shape, rng)
        .map_err(|e| crate::ZkError::Setup(e.to_string()))
}

/// Prove against an existing (canonical) proving key. The verifying key is
/// NOT emitted — proving must not let the prover choose the VK. Returns the
/// commitment (engine cross-checks this) and the compressed proof bytes.
pub fn prove_with_key<R: RngCore + CryptoRng>(
    pk: &ark_groth16::ProvingKey<Bn254>,
    circuit: &CommitmentPreimageCircuit,
    rng: &mut R,
) -> Result<(Fr, Vec<u8>), crate::ZkError> {
    let commitment = compute_commitment_native(circuit);
    let proof = Groth16::<Bn254>::prove(pk, circuit.clone(), rng)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;
    let mut proof_bytes = Vec::new();
    proof
        .serialize_with_mode(&mut proof_bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok((commitment, proof_bytes))
}

/// Serialize a verifying key to compressed bytes (for embedding/distribution).
pub fn serialize_vk(vk: &ark_groth16::VerifyingKey<Bn254>) -> Result<Vec<u8>, crate::ZkError> {
    let mut bytes = Vec::new();
    vk.serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok(bytes)
}

/// Serialize a proving key to compressed bytes (for distribution to provers).
pub fn serialize_pk(pk: &ark_groth16::ProvingKey<Bn254>) -> Result<Vec<u8>, crate::ZkError> {
    let mut bytes = Vec::new();
    pk.serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok(bytes)
}

/// Deserialize a verifying key from compressed bytes.
pub fn deserialize_vk(
    vk_bytes: &[u8],
) -> Result<ark_groth16::VerifyingKey<Bn254>, crate::ZkError> {
    ark_groth16::VerifyingKey::<Bn254>::deserialize_with_mode(vk_bytes, Compress::Yes, Validate::Yes)
        .map_err(|e| crate::ZkError::Serialize(format!("deserialize vk: {e}")))
}

/// Deserialize a proving key from compressed bytes.
pub fn deserialize_pk(
    pk_bytes: &[u8],
) -> Result<ark_groth16::ProvingKey<Bn254>, crate::ZkError> {
    ark_groth16::ProvingKey::<Bn254>::deserialize_with_mode(pk_bytes, Compress::Yes, Validate::Yes)
        .map_err(|e| crate::ZkError::Serialize(format!("deserialize pk: {e}")))
}

/// **DEMO / FIXTURE USE ONLY — NOT a verification primitive.**
///
/// Runs `circuit_specific_setup` *per proof* with the caller's RNG, so the
/// prover generates its own verifying key. A proof produced this way is
/// only meaningful when verified against the VK emitted alongside it — which
/// is exactly the unsoundness a verifier must avoid: a malicious prover can
/// run setup for a trivially-satisfiable circuit and emit a `(vk, proof)`
/// pair that verifies for an arbitrary commitment.
///
/// The engine MUST NOT use this. It exists for CLI fixtures and WASM demos
/// where there is no canonical VK to verify against. Soundness at ingestion
/// comes from [`generate_keys`] (run once) + [`prove_with_key`] +
/// [`verify_proof_with_vk`] (against the pinned canonical VK).
pub fn setup_and_prove<R: RngCore + CryptoRng>(
    circuit: &CommitmentPreimageCircuit,
    rng: &mut R,
) -> Result<SingleOrderProof, crate::ZkError> {
    let (pk, vk) = generate_keys(rng)?;
    let (commitment, proof_bytes) = prove_with_key(&pk, circuit, rng)?;
    let vk_bytes = serialize_vk(&vk)?;

    // Self-check the proof verifies under the just-generated VK. This is a
    // sanity check on the prover, NOT a security guarantee — the VK is
    // prover-chosen here.
    if !verify_proof(&vk_bytes, &proof_bytes, commitment)? {
        return Err(crate::ZkError::Verify);
    }

    Ok(SingleOrderProof {
        commitment,
        proof_bytes,
        vk_bytes,
    })
}

/// Verify a proof against a deserialized, **canonical** verifying key. This
/// is the sound entrypoint: the VK is pinned by the verifier (loaded once at
/// boot), never supplied by the prover. The proof is bound to `commitment`
/// via the public input.
pub fn verify_proof_with_vk(
    vk: &ark_groth16::VerifyingKey<Bn254>,
    proof_bytes: &[u8],
    commitment: Fr,
) -> Result<bool, crate::ZkError> {
    let proof = ark_groth16::Proof::<Bn254>::deserialize_with_mode(
        proof_bytes,
        Compress::Yes,
        Validate::Yes,
    )
    .map_err(|e| crate::ZkError::Serialize(format!("deserialize proof: {e}")))?;

    let pvk = ark_groth16::prepare_verifying_key(vk);
    let valid = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &[commitment], &proof)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;

    Ok(valid)
}

/// Verify a proof against a verifying key supplied as bytes.
///
/// SECURITY: the caller is responsible for the provenance of `vk_bytes`. If
/// the bytes came from the prover, the result is meaningless (see
/// [`setup_and_prove`]). Pass the canonical VK only. The engine uses
/// [`verify_proof_with_vk`] with a VK it loaded at boot.
pub fn verify_proof(
    vk_bytes: &[u8],
    proof_bytes: &[u8],
    commitment: Fr,
) -> Result<bool, crate::ZkError> {
    let vk = deserialize_vk(vk_bytes)?;
    verify_proof_with_vk(&vk, proof_bytes, commitment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::decimal_to_scalar;
    use crate::pedersen::bytes_to_scalar;
    use ark_ff::{One, Zero};
    use ark_std::rand::rngs::StdRng;
    use ark_std::rand::SeedableRng;
    use rust_decimal::Decimal;

    fn fixed_rng() -> StdRng {
        StdRng::from_seed([42u8; 32])
    }

    fn sample_circuit() -> CommitmentPreimageCircuit {
        let trader_id = crate::pedersen::derive_trader_id(b"alice").unwrap();
        let salt = bytes_to_scalar(&[0x22u8; 32]);
        CommitmentPreimageCircuit {
            trader_id,
            side: Fr::zero(),
            limit_price: decimal_to_scalar(Decimal::from(100)).unwrap(),
            size: decimal_to_scalar(Decimal::from(10)).unwrap(),
            salt,
        }
    }

    #[test]
    fn proof_verifies() {
        let circuit = sample_circuit();
        let mut rng = fixed_rng();
        let result = setup_and_prove(&circuit, &mut rng).unwrap();
        assert!(verify_proof(&result.vk_bytes, &result.proof_bytes, result.commitment).unwrap());
    }

    #[test]
    fn proof_rejects_wrong_commitment() {
        let circuit = sample_circuit();
        let mut rng = fixed_rng();
        let result = setup_and_prove(&circuit, &mut rng).unwrap();
        let wrong_commitment = result.commitment + Fr::one();
        assert!(!verify_proof(&result.vk_bytes, &result.proof_bytes, wrong_commitment).unwrap());
    }

    #[test]
    fn deterministic_with_same_rng_seed() {
        let circuit = sample_circuit();
        let mut rng1 = fixed_rng();
        let mut rng2 = fixed_rng();
        let r1 = setup_and_prove(&circuit, &mut rng1).unwrap();
        let r2 = setup_and_prove(&circuit, &mut rng2).unwrap();
        assert_eq!(r1.commitment, r2.commitment);
        assert_eq!(r1.proof_bytes, r2.proof_bytes);
        assert_eq!(r1.vk_bytes, r2.vk_bytes);
    }

    /// A VK generated once by `generate_keys` (independent of any order's
    /// witness, using a zero shape) verifies a real order proof produced
    /// against the matching proving key. This is the canonical-VK flow the
    /// engine uses.
    #[test]
    fn canonical_keys_prove_and_verify() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();

        let circuit = sample_circuit();
        let (commitment, proof_bytes) = prove_with_key(&pk, &circuit, &mut rng).unwrap();

        assert!(verify_proof_with_vk(&vk, &proof_bytes, commitment).unwrap());
        // Tampering with the commitment public input must fail.
        assert!(!verify_proof_with_vk(&vk, &proof_bytes, commitment + Fr::one()).unwrap());
    }

    /// SOUNDNESS REGRESSION (issue #158): a proof produced under one VK must
    /// NOT verify against a different, canonical VK. The old per-proof-setup
    /// path let a prover ship its own VK; pinning a canonical VK closes that
    /// hole. A proof minted under `setup_and_prove`'s prover-chosen VK is
    /// rejected by an independently generated canonical VK.
    #[test]
    fn proof_under_foreign_vk_rejected_by_canonical_vk() {
        let circuit = sample_circuit();

        // Prover-chosen keys (the attack surface): VK travels with the proof.
        let mut prover_rng = StdRng::from_seed([7u8; 32]);
        let attacker = setup_and_prove(&circuit, &mut prover_rng).unwrap();

        // Canonical keys: generated once by the operator with different RNG.
        let mut canonical_rng = StdRng::from_seed([99u8; 32]);
        let (_pk, canonical_vk) = generate_keys(&mut canonical_rng).unwrap();

        // The attacker's proof verifies under its own VK (that's the trap)...
        assert!(verify_proof(
            &attacker.vk_bytes,
            &attacker.proof_bytes,
            attacker.commitment
        )
        .unwrap());
        // ...but is rejected by the pinned canonical VK.
        assert!(!verify_proof_with_vk(
            &canonical_vk,
            &attacker.proof_bytes,
            attacker.commitment
        )
        .unwrap());
    }

    #[test]
    fn vk_pk_round_trip_serialization() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();
        let vk_bytes = serialize_vk(&vk).unwrap();
        let pk_bytes = serialize_pk(&pk).unwrap();

        let vk2 = deserialize_vk(&vk_bytes).unwrap();
        let pk2 = deserialize_pk(&pk_bytes).unwrap();

        // A proof under the round-tripped pk verifies under the round-tripped vk.
        let circuit = sample_circuit();
        let (commitment, proof_bytes) = prove_with_key(&pk2, &circuit, &mut rng).unwrap();
        assert!(verify_proof_with_vk(&vk2, &proof_bytes, commitment).unwrap());
    }
}
