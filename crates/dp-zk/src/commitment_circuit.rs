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

pub fn setup_and_prove<R: RngCore + CryptoRng>(
    circuit: &CommitmentPreimageCircuit,
    rng: &mut R,
) -> Result<SingleOrderProof, crate::ZkError> {
    let commitment = compute_commitment_native(circuit);

    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit.clone(), rng)
        .map_err(|e| crate::ZkError::Setup(e.to_string()))?;

    let proof = Groth16::<Bn254>::prove(&pk, circuit.clone(), rng)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;

    let pvk =
        Groth16::<Bn254>::process_vk(&vk).map_err(|e| crate::ZkError::Setup(e.to_string()))?;

    let valid = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &[commitment], &proof)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;

    if !valid {
        return Err(crate::ZkError::Verify);
    }

    let mut proof_bytes = Vec::new();
    proof
        .serialize_with_mode(&mut proof_bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;

    let mut vk_bytes = Vec::new();
    vk.serialize_with_mode(&mut vk_bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;

    Ok(SingleOrderProof {
        commitment,
        proof_bytes,
        vk_bytes,
    })
}

pub fn verify_proof(
    vk_bytes: &[u8],
    proof_bytes: &[u8],
    commitment: Fr,
) -> Result<bool, crate::ZkError> {
    let vk = ark_groth16::VerifyingKey::<Bn254>::deserialize_with_mode(
        vk_bytes,
        Compress::Yes,
        Validate::Yes,
    )
    .map_err(|e| crate::ZkError::Serialize(format!("deserialize vk: {e}")))?;

    let proof = ark_groth16::Proof::<Bn254>::deserialize_with_mode(
        proof_bytes,
        Compress::Yes,
        Validate::Yes,
    )
    .map_err(|e| crate::ZkError::Serialize(format!("deserialize proof: {e}")))?;

    let pvk = ark_groth16::prepare_verifying_key(&vk);
    let valid = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &[commitment], &proof)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;

    Ok(valid)
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
}
