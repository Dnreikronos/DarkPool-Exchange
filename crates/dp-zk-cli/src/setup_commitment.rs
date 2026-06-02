//! `setup-commitment-circuit` subcommand: one-time trusted setup for the
//! per-order `CommitmentPreimageCircuit`.
//!
//! Runs Groth16 `circuit_specific_setup` ONCE and writes the canonical
//! proving/verifying key pair to disk. The verifying key (`commitment_vk.bin`)
//! is the value the engine pins — see `dp-engine`'s `Groth16OrderProofVerifier`
//! and ADR 0001. Provers load the proving key (`commitment_pk.bin`).
//!
//! This is the fix for issue #158: pinning a single canonical VK is what makes
//! per-order proof verification sound. The prover must NOT be allowed to ship
//! its own VK.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use dp_zk::commitment_circuit::{generate_keys, serialize_pk, serialize_vk};

pub const PK_FILENAME: &str = "commitment_pk.bin";
pub const VK_FILENAME: &str = "commitment_vk.bin";

#[derive(Parser, Debug)]
pub struct SetupCommitmentArgs {
    /// Output directory for the key pair. Created if it does not exist.
    #[arg(long)]
    pub out: PathBuf,
    /// RNG seed for deterministic key generation (reproducible fixtures /
    /// CI). Omit for a fresh OS-RNG ceremony.
    #[arg(long)]
    pub seed: Option<u64>,
}

pub fn run_setup_commitment(args: SetupCommitmentArgs) -> ExitCode {
    match generate_and_write(&args.out, args.seed) {
        Ok((pk_path, vk_path)) => {
            eprintln!("wrote proving key:   {}", pk_path.display());
            eprintln!("wrote verifying key: {}", vk_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("setup-commitment-circuit: {e}");
            ExitCode::from(4)
        }
    }
}

/// Generate the canonical key pair and write both files. Returns the written
/// paths on success. Factored out of [`run_setup_commitment`] so it is
/// testable without process IO.
pub fn generate_and_write(
    out: &std::path::Path,
    seed: Option<u64>,
) -> Result<(PathBuf, PathBuf), String> {
    let mut rng = make_rng(seed);
    let (pk, vk) = generate_keys(&mut rng).map_err(|e| format!("key generation: {e}"))?;

    let pk_bytes = serialize_pk(&pk).map_err(|e| format!("serialize proving key: {e}"))?;
    let vk_bytes = serialize_vk(&vk).map_err(|e| format!("serialize verifying key: {e}"))?;

    std::fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let pk_path = out.join(PK_FILENAME);
    let vk_path = out.join(VK_FILENAME);
    std::fs::write(&pk_path, &pk_bytes).map_err(|e| format!("write {}: {e}", pk_path.display()))?;
    std::fs::write(&vk_path, &vk_bytes).map_err(|e| format!("write {}: {e}", vk_path.display()))?;

    Ok((pk_path, vk_path))
}

fn make_rng(seed: Option<u64>) -> ark_std::rand::rngs::StdRng {
    use ark_std::rand::SeedableRng;
    match seed {
        Some(s) => {
            let mut full_seed = [0u8; 32];
            full_seed[..8].copy_from_slice(&s.to_le_bytes());
            ark_std::rand::rngs::StdRng::from_seed(full_seed)
        }
        None => ark_std::rand::rngs::StdRng::from_entropy(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_zk::commitment_circuit::{
        deserialize_pk, deserialize_vk, prove_with_key, verify_proof_with_vk,
        CommitmentPreimageCircuit,
    };
    use dp_zk::encoding::decimal_to_scalar;
    use dp_zk::pedersen::bytes_to_scalar;
    use ark_bn254::Fr;
    use ark_ff::Zero;
    use rust_decimal::Decimal;

    fn sample_circuit() -> CommitmentPreimageCircuit {
        let trader_id = dp_zk::pedersen::derive_trader_id(b"alice").unwrap();
        CommitmentPreimageCircuit {
            trader_id,
            side: Fr::zero(),
            limit_price: decimal_to_scalar(Decimal::from(100)).unwrap(),
            size: decimal_to_scalar(Decimal::from(10)).unwrap(),
            salt: bytes_to_scalar(&[0x22u8; 32]),
        }
    }

    #[test]
    fn writes_both_keys_and_they_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let (pk_path, vk_path) = generate_and_write(dir.path(), Some(42)).unwrap();
        assert!(pk_path.exists());
        assert!(vk_path.exists());

        let pk = deserialize_pk(&std::fs::read(&pk_path).unwrap()).unwrap();
        let vk = deserialize_vk(&std::fs::read(&vk_path).unwrap()).unwrap();

        let circuit = sample_circuit();
        let mut rng = make_rng(Some(7));
        let (commitment, proof) = prove_with_key(&pk, &circuit, &mut rng).unwrap();
        assert!(verify_proof_with_vk(&vk, &proof, commitment).unwrap());
    }

    #[test]
    fn deterministic_with_seed() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        generate_and_write(a.path(), Some(99)).unwrap();
        generate_and_write(b.path(), Some(99)).unwrap();
        let vk_a = std::fs::read(a.path().join(VK_FILENAME)).unwrap();
        let vk_b = std::fs::read(b.path().join(VK_FILENAME)).unwrap();
        assert_eq!(vk_a, vk_b);
    }
}
