use std::path::Path;

use ark_bn254::Bn254;
use ark_groth16::{prepare_verifying_key, PreparedVerifyingKey};
use dp_zk::commitment_circuit::{
    deserialize_vk, verify_proof_with_processed_vk, OrderProofPublics,
};

/// Verifies a per-order commitment proof against engine-derived public inputs.
///
/// The caller must derive `OrderProofPublics` from decrypted order fields and
/// the client salt; never trust public inputs supplied by the prover.
pub trait OrderProofVerifier: Send + Sync {
    fn verify(&self, proof: &[u8], publics: &OrderProofPublics) -> Result<(), String>;
}

/// Groth16 verifier pinned to one canonical commitment-circuit verifying key.
pub struct Groth16OrderProofVerifier {
    pvk: PreparedVerifyingKey<Bn254>,
}

impl Groth16OrderProofVerifier {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let vk = deserialize_vk(bytes).map_err(|e| format!("deserialize order-proof VK: {e}"))?;
        Ok(Self {
            pvk: prepare_verifying_key(&vk),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| format!("read order-proof VK {}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }
}

impl OrderProofVerifier for Groth16OrderProofVerifier {
    fn verify(&self, proof: &[u8], publics: &OrderProofPublics) -> Result<(), String> {
        match verify_proof_with_processed_vk(&self.pvk, proof, publics) {
            Ok(true) => Ok(()),
            Ok(false) => Err("order proof rejected by canonical VK".to_string()),
            Err(e) => Err(format!("verify order proof: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ark_bn254::Fr;
    use dp_zk::commitment_circuit::{
        generate_keys, prove_with_key, serialize_vk, CommitmentPreimageCircuit,
    };
    use dp_zk::pedersen::{bytes_to_scalar, derive_trader_id};
    use rand::SeedableRng;

    fn proof_fixture() -> (OrderProofPublics, Vec<u8>, Vec<u8>) {
        let mut setup_rng = rand::rngs::StdRng::from_seed([7u8; 32]);
        let (pk, vk) = generate_keys(&mut setup_rng).unwrap();
        let vk_bytes = serialize_vk(&vk).unwrap();
        let circuit = CommitmentPreimageCircuit {
            trader_id: derive_trader_id(b"alice").unwrap(),
            trader_addr: bytes_to_scalar(b"alice"),
            side: Fr::from(0u64),
            limit_price: Fr::from(100u64),
            size: Fr::from(2u64),
            salt: Fr::from(9u64),
        };
        let mut prove_rng = rand::rngs::StdRng::from_seed([8u8; 32]);
        let (publics, proof) = prove_with_key(&pk, &circuit, &mut prove_rng).unwrap();
        (publics, proof, vk_bytes)
    }

    #[test]
    fn from_file_loads_verifier() {
        let (publics, proof, vk_bytes) = proof_fixture();
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), vk_bytes).unwrap();

        let verifier = Groth16OrderProofVerifier::from_file(file.path()).unwrap();

        verifier.verify(&proof, &publics).unwrap();
    }

    #[test]
    fn from_file_reports_read_error() {
        let Err(err) = Groth16OrderProofVerifier::from_file("__missing_order_vk__.bin") else {
            panic!("missing VK path should fail");
        };

        assert!(err.contains("read order-proof VK"), "got {err}");
    }

    #[test]
    fn verify_rejects_mismatched_public_inputs() {
        let (mut publics, proof, vk_bytes) = proof_fixture();
        let verifier = Groth16OrderProofVerifier::from_bytes(&vk_bytes).unwrap();
        publics.nullifier += Fr::from(1u64);

        let err = verifier.verify(&proof, &publics).unwrap_err();

        assert_eq!(err, "order proof rejected by canonical VK");
    }
}
