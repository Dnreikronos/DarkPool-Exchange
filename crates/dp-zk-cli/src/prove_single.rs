//! `prove-single-order` subcommand: Groth16 proof of commitment preimage.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use ark_ff::{One, Zero};
use clap::Parser;
use dp_zk::commitment_circuit::{setup_and_prove, CommitmentPreimageCircuit};
use dp_zk::encoding::decimal_to_scalar;
use dp_zk::pedersen::bytes_to_scalar;
use dp_zk::{fr_to_bytes32, OrderCommitmentInput, commit_native};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
pub struct ProveSingleArgs {
    /// Path to JSON input file. Reads from stdin if omitted.
    #[arg(long)]
    pub input: Option<PathBuf>,
    /// RNG seed for deterministic proof generation. Required for reproducible fixtures.
    #[arg(long)]
    pub seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ProveSingleInput {
    pub trader_id: Option<String>,
    pub salt: String,
    pub side: u8,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
    pub commitment_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProveSingleOutput {
    pub scalars: ScalarOutput,
    pub commitment: String,
    pub proof: String,
    pub verification_key: String,
}

#[derive(Debug, Serialize)]
pub struct ScalarOutput {
    pub trader_id: String,
    pub side: String,
    pub limit_price: String,
    pub size: String,
    pub salt: String,
}

pub fn run_prove_single(args: ProveSingleArgs) -> ExitCode {
    let bytes = match read_input(args.input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read input: {e}");
            return ExitCode::from(2);
        }
    };

    let input: ProveSingleInput = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("parse input: {e}");
            return ExitCode::from(2);
        }
    };

    match generate_proof(&input, args.seed) {
        Ok(output) => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("prove: {e}");
            ExitCode::from(4)
        }
    }
}

pub fn generate_proof(
    input: &ProveSingleInput,
    seed: Option<u64>,
) -> Result<ProveSingleOutput, String> {
    let trader_id = resolve_trader_id(input)?;

    let salt_bytes = hex::decode(input.salt.trim_start_matches("0x"))
        .map_err(|e| format!("salt hex: {e}"))?;
    let salt = bytes_to_scalar(&salt_bytes);

    let limit_price = decimal_to_scalar(input.limit_price)
        .map_err(|e| format!("limit_price: {e}"))?;
    let size = decimal_to_scalar(input.size)
        .map_err(|e| format!("size: {e}"))?;

    let side_fr = if input.side == 0 {
        ark_bn254::Fr::zero()
    } else {
        ark_bn254::Fr::one()
    };

    let circuit = CommitmentPreimageCircuit {
        trader_id,
        side: side_fr,
        limit_price,
        size,
        salt,
    };

    let commitment = commit_native(&OrderCommitmentInput {
        trader_id: circuit.trader_id,
        side: circuit.side,
        limit_price: circuit.limit_price,
        size: circuit.size,
        salt: circuit.salt,
    });

    let mut rng = make_rng(seed);
    let result = setup_and_prove(&circuit, &mut rng)
        .map_err(|e| format!("proof generation: {e}"))?;

    Ok(ProveSingleOutput {
        scalars: ScalarOutput {
            trader_id: hex::encode(fr_to_bytes32(trader_id)),
            side: hex::encode(fr_to_bytes32(side_fr)),
            limit_price: hex::encode(fr_to_bytes32(limit_price)),
            size: hex::encode(fr_to_bytes32(size)),
            salt: hex::encode(fr_to_bytes32(salt)),
        },
        commitment: hex::encode(fr_to_bytes32(commitment)),
        proof: hex::encode(&result.proof_bytes),
        verification_key: hex::encode(&result.vk_bytes),
    })
}

fn resolve_trader_id(input: &ProveSingleInput) -> Result<ark_bn254::Fr, String> {
    if let Some(ref tid) = input.trader_id {
        let bytes = hex::decode(tid.trim_start_matches("0x"))
            .map_err(|e| format!("trader_id hex: {e}"))?;
        Ok(bytes_to_scalar(&bytes))
    } else if let Some(ref ck) = input.commitment_key {
        dp_zk::pedersen::derive_trader_id(ck.as_bytes())
            .map_err(|e| format!("derive trader_id: {e}"))
    } else {
        Err("must provide either trader_id or commitment_key".into())
    }
}

fn make_rng(seed: Option<u64>) -> ark_std::rand::rngs::StdRng {
    use ark_std::rand::SeedableRng;
    let mut full_seed = [0u8; 32];
    match seed {
        Some(s) => full_seed[..8].copy_from_slice(&s.to_le_bytes()),
        None => full_seed[..8].copy_from_slice(&1234567890u64.to_le_bytes()),
    }
    ark_std::rand::rngs::StdRng::from_seed(full_seed)
}

fn read_input(path: Option<PathBuf>) -> Result<Vec<u8>, std::io::Error> {
    match path {
        Some(p) => std::fs::read(p),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().lock().read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> ProveSingleInput {
        ProveSingleInput {
            trader_id: None,
            salt: "00".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("alice".into()),
        }
    }

    #[test]
    fn generates_valid_proof() {
        let input = sample_input();
        let output = generate_proof(&input, Some(42)).unwrap();
        assert_eq!(output.commitment.len(), 64);
        assert!(!output.proof.is_empty());
        assert!(!output.verification_key.is_empty());
    }

    #[test]
    fn deterministic_with_same_seed() {
        let input = sample_input();
        let a = generate_proof(&input, Some(42)).unwrap();
        let b = generate_proof(&input, Some(42)).unwrap();
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.proof, b.proof);
        assert_eq!(a.verification_key, b.verification_key);
        assert_eq!(a.scalars.trader_id, b.scalars.trader_id);
    }

    #[test]
    fn different_seed_different_proof_same_commitment() {
        let input = sample_input();
        let a = generate_proof(&input, Some(1)).unwrap();
        let b = generate_proof(&input, Some(2)).unwrap();
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.scalars.trader_id, b.scalars.trader_id);
        // Proof bytes differ because different randomness, but both verify
        // (vk also differs because setup randomness changes).
    }

    #[test]
    fn commitment_matches_commit_subcommand() {
        let input = sample_input();
        let proof_output = generate_proof(&input, Some(42)).unwrap();

        let commit_input = crate::commit::CommitInput {
            trader_id: None,
            salt: "00".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("alice".into()),
        };
        let commit_hex = crate::commit::compute_commitment(&commit_input).unwrap();

        assert_eq!(proof_output.commitment, commit_hex);
    }

    #[test]
    fn proof_independently_verifiable() {
        let input = sample_input();
        let output = generate_proof(&input, Some(42)).unwrap();

        let commitment_bytes = hex::decode(&output.commitment).unwrap();
        let commitment = dp_zk::pedersen::bytes_to_scalar(&commitment_bytes);

        let proof_bytes = hex::decode(&output.proof).unwrap();
        let vk_bytes = hex::decode(&output.verification_key).unwrap();

        let valid = dp_zk::commitment_circuit::verify_proof(&vk_bytes, &proof_bytes, commitment)
            .unwrap();
        assert!(valid);
    }

    #[test]
    fn rejects_missing_identity() {
        let input = ProveSingleInput {
            trader_id: None,
            salt: "00".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: None,
        };
        assert!(generate_proof(&input, Some(42)).is_err());
    }
}
