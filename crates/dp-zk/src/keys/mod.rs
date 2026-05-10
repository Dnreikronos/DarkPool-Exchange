//! Proving/verifying key serde + on-disk metadata.

pub mod solidity;
pub use solidity::{fq_to_hex, vk_to_solidity, SolidityVk};

use std::fs;
use std::path::Path;

use ark_bn254::Bn254;
use ark_groth16::{ProvingKey, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ZkError, CIRCUIT_VERSION};

/// On-disk JSON sidecar describing the keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub circuit_version: String,
    pub batch_size: u32,
    pub proving_key_sha256: String,
    pub verifying_key_sha256: String,
}

impl KeyMetadata {
    pub fn check_compatible(&self) -> Result<(), ZkError> {
        if self.circuit_version != CIRCUIT_VERSION {
            return Err(ZkError::VersionMismatch {
                keys: self.circuit_version.clone(),
                current: CIRCUIT_VERSION.to_string(),
            });
        }
        Ok(())
    }
}

pub struct ProvingKeyBytes(pub Vec<u8>);
pub struct VerifyingKeyBytes(pub Vec<u8>);

pub fn serialize_pk(pk: &ProvingKey<Bn254>) -> Result<Vec<u8>, ZkError> {
    let mut buf = Vec::with_capacity(pk.serialized_size(Compress::Yes));
    pk.serialize_with_mode(&mut buf, Compress::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;
    Ok(buf)
}

pub fn deserialize_pk(bytes: &[u8]) -> Result<ProvingKey<Bn254>, ZkError> {
    ProvingKey::<Bn254>::deserialize_with_mode(bytes, Compress::Yes, Validate::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))
}

pub fn serialize_vk(vk: &VerifyingKey<Bn254>) -> Result<Vec<u8>, ZkError> {
    let mut buf = Vec::with_capacity(vk.serialized_size(Compress::Yes));
    vk.serialize_with_mode(&mut buf, Compress::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;
    Ok(buf)
}

pub fn deserialize_vk(bytes: &[u8]) -> Result<VerifyingKey<Bn254>, ZkError> {
    VerifyingKey::<Bn254>::deserialize_with_mode(bytes, Compress::Yes, Validate::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))
}

pub fn write_keys_to_dir(
    dir: &Path,
    pk: &ProvingKey<Bn254>,
    vk: &VerifyingKey<Bn254>,
    batch_size: u32,
) -> Result<KeyMetadata, ZkError> {
    fs::create_dir_all(dir)?;
    let pk_bytes = serialize_pk(pk)?;
    let vk_bytes = serialize_vk(vk)?;
    fs::write(dir.join("proving_key.bin"), &pk_bytes)?;
    fs::write(dir.join("verifying_key.bin"), &vk_bytes)?;
    let meta = KeyMetadata {
        circuit_version: CIRCUIT_VERSION.to_string(),
        batch_size,
        proving_key_sha256: hex::encode(Sha256::digest(&pk_bytes)),
        verifying_key_sha256: hex::encode(Sha256::digest(&vk_bytes)),
    };
    fs::write(
        dir.join("keys_metadata.json"),
        serde_json::to_vec_pretty(&meta).map_err(|e| ZkError::Serialize(e.to_string()))?,
    )?;
    Ok(meta)
}

pub fn read_metadata(dir: &Path) -> Result<KeyMetadata, ZkError> {
    let raw = fs::read(dir.join("keys_metadata.json"))?;
    let meta: KeyMetadata =
        serde_json::from_slice(&raw).map_err(|e| ZkError::Serialize(e.to_string()))?;
    Ok(meta)
}

pub fn read_pk(dir: &Path) -> Result<ProvingKey<Bn254>, ZkError> {
    let raw = fs::read(dir.join("proving_key.bin"))?;
    deserialize_pk(&raw)
}

pub fn read_vk(dir: &Path) -> Result<VerifyingKey<Bn254>, ZkError> {
    let raw = fs::read(dir.join("verifying_key.bin"))?;
    deserialize_vk(&raw)
}
