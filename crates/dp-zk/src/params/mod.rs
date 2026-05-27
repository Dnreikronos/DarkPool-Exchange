//! HyperNova IVC params persistence: stores `public_params.bin` + `params_metadata.json`.
//!
//! Only the prover params are serialized (the CCS/R1CS needed for the
//! verifier params can be re-derived from the circuit at load time via
//! `HN::pp_deserialize_with_mode` / `HN::vp_deserialize_with_mode`).

use std::fs;
use std::path::Path;

use ark_serialize::{CanonicalSerialize, Compress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::folding::HyperNovaPublicParams;
use crate::ZkError;
use crate::CIRCUIT_VERSION;

const PARAMS_FILE: &str = "public_params.bin";
const META_FILE: &str = "params_metadata.json";

/// Metadata written alongside the serialized params.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamsMetadata {
    pub circuit_version: String,
    pub sha256: String,
    pub batch_size: usize,
}

/// Serialize `params.prover` to `<dir>/public_params.bin` and write a
/// companion `<dir>/params_metadata.json` with a SHA-256 checksum and
/// circuit version string.
///
/// On a subsequent load the caller should verify `metadata.circuit_version`
/// matches [`CIRCUIT_VERSION`] and re-compute the SHA-256 before trusting
/// the bytes.
pub fn save(
    params: &HyperNovaPublicParams,
    dir: &Path,
    batch_size: usize,
) -> Result<ParamsMetadata, ZkError> {
    fs::create_dir_all(dir)?;

    let mut buf = Vec::new();
    params
        .prover
        .serialize_with_mode(&mut buf, Compress::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;

    let sha256 = hex::encode(Sha256::digest(&buf));
    fs::write(dir.join(PARAMS_FILE), &buf)?;

    let meta = ParamsMetadata {
        circuit_version: CIRCUIT_VERSION.to_string(),
        sha256: sha256.clone(),
        batch_size,
    };
    let meta_json =
        serde_json::to_string_pretty(&meta).map_err(|e| ZkError::Serialize(e.to_string()))?;
    fs::write(dir.join(META_FILE), meta_json)?;

    Ok(meta)
}

/// Load the [`ParamsMetadata`] from `<dir>/params_metadata.json` without
/// deserializing the (potentially large) params binary.
pub fn load_metadata(dir: &Path) -> Result<ParamsMetadata, ZkError> {
    let raw = fs::read_to_string(dir.join(META_FILE))?;
    serde_json::from_str(&raw).map_err(|e| ZkError::Serialize(e.to_string()))
}

/// Read the raw prover-param bytes from `<dir>/public_params.bin`.
/// Callers that need the deserialized form should use
/// `HN::pp_deserialize_with_mode` on the returned slice.
pub fn load_prover_bytes(dir: &Path) -> Result<Vec<u8>, ZkError> {
    let bytes = fs::read(dir.join(PARAMS_FILE))?;
    Ok(bytes)
}
