use std::path::Path;

use dp_zk::commitment_circuit::{deserialize_vk, verify_proof, OrderProofPublics};

/// Verifies a per-order commitment proof against engine-derived public inputs.
///
/// The caller must derive `OrderProofPublics` from decrypted order fields and
/// the client salt; never trust public inputs supplied by the prover.
pub trait OrderProofVerifier: Send + Sync {
    fn verify(&self, proof: &[u8], publics: &OrderProofPublics) -> Result<(), String>;
}

/// Groth16 verifier pinned to one canonical commitment-circuit verifying key.
pub struct Groth16OrderProofVerifier {
    vk_bytes: Vec<u8>,
}

impl Groth16OrderProofVerifier {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        deserialize_vk(bytes).map_err(|e| format!("deserialize order-proof VK: {e}"))?;
        Ok(Self {
            vk_bytes: bytes.to_vec(),
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
        match verify_proof(&self.vk_bytes, proof, publics) {
            Ok(true) => Ok(()),
            Ok(false) => Err("order proof rejected by canonical VK".to_string()),
            Err(e) => Err(format!("verify order proof: {e}")),
        }
    }
}
