//! `commit` subcommand: compute Poseidon commitment for a single order.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use dp_zk::pedersen::{bytes_to_scalar, commit_native, OrderCommitmentInput};
use dp_zk::encoding::decimal_to_scalar;
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Parser, Debug)]
pub struct CommitArgs {
    /// Path to JSON input file. Reads from stdin if omitted.
    #[arg(long)]
    pub input: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct CommitInput {
    /// Hex-encoded 32-byte trader id (or derived from commitment_key if omitted).
    pub trader_id: Option<String>,
    /// Hex-encoded salt bytes.
    pub salt: String,
    /// 0 = bid, 1 = ask.
    pub side: u8,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
    /// If provided and trader_id is absent, trader_id = poseidon(commitment_key).
    pub commitment_key: Option<String>,
}

pub fn run_commit(args: CommitArgs) -> ExitCode {
    let bytes = match read_input(args.input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read input: {e}");
            return ExitCode::from(2);
        }
    };

    let input: CommitInput = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("parse input: {e}");
            return ExitCode::from(2);
        }
    };

    match compute_commitment(&input) {
        Ok(hex) => {
            println!("{hex}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("commitment: {e}");
            ExitCode::from(2)
        }
    }
}

pub fn compute_commitment(input: &CommitInput) -> Result<String, String> {
    let trader_id = resolve_trader_id(input)?;

    let salt_bytes = hex::decode(input.salt.trim_start_matches("0x"))
        .map_err(|e| format!("salt hex: {e}"))?;
    let salt = bytes_to_scalar(&salt_bytes);

    let limit_price = decimal_to_scalar(input.limit_price)
        .map_err(|e| format!("limit_price: {e}"))?;
    let size = decimal_to_scalar(input.size)
        .map_err(|e| format!("size: {e}"))?;

    let ci = OrderCommitmentInput {
        trader_id,
        side: if input.side == 0 {
            ark_ff::Zero::zero()
        } else {
            ark_ff::One::one()
        },
        limit_price,
        size,
        salt,
    };

    let commitment = commit_native(&ci);
    Ok(hex::encode(dp_zk::fr_to_bytes32(commitment)))
}

fn resolve_trader_id(input: &CommitInput) -> Result<ark_bn254::Fr, String> {
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

    #[test]
    fn deterministic_output() {
        let input = CommitInput {
            trader_id: None,
            salt: "22".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("bid_key".into()),
        };
        let a = compute_commitment(&input).unwrap();
        let b = compute_commitment(&input).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes hex
    }

    #[test]
    fn explicit_trader_id_matches_derived() {
        let ck = "bid_key";
        let tid_bytes = dp_zk::pedersen::derive_trader_id_bytes(ck.as_bytes());
        let tid_hex = hex::encode(tid_bytes);

        let with_ck = CommitInput {
            trader_id: None,
            salt: "11".repeat(32),
            side: 0,
            limit_price: Decimal::from(50),
            size: Decimal::from(5),
            commitment_key: Some(ck.into()),
        };
        let with_tid = CommitInput {
            trader_id: Some(tid_hex),
            salt: "11".repeat(32),
            side: 0,
            limit_price: Decimal::from(50),
            size: Decimal::from(5),
            commitment_key: None,
        };
        assert_eq!(
            compute_commitment(&with_ck).unwrap(),
            compute_commitment(&with_tid).unwrap(),
        );
    }

    #[test]
    fn different_salt_different_commitment() {
        let base = CommitInput {
            trader_id: None,
            salt: "aa".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("key".into()),
        };
        let other = CommitInput {
            salt: "bb".repeat(32),
            ..CommitInput {
                trader_id: None,
                salt: String::new(),
                side: 0,
                limit_price: Decimal::from(100),
                size: Decimal::from(10),
                commitment_key: Some("key".into()),
            }
        };
        assert_ne!(
            compute_commitment(&base).unwrap(),
            compute_commitment(&other).unwrap(),
        );
    }

    #[test]
    fn rejects_missing_trader_id_and_key() {
        let input = CommitInput {
            trader_id: None,
            salt: "00".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: None,
        };
        assert!(compute_commitment(&input).is_err());
    }

    #[test]
    fn rejects_bad_salt_hex() {
        let input = CommitInput {
            trader_id: None,
            salt: "not-hex".into(),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("k".into()),
        };
        assert!(compute_commitment(&input).is_err());
    }

    #[test]
    fn snapshot_known_vector() {
        let input = CommitInput {
            trader_id: None,
            salt: "00".repeat(32),
            side: 0,
            limit_price: Decimal::from(100),
            size: Decimal::from(10),
            commitment_key: Some("alice".into()),
        };
        let hex = compute_commitment(&input).unwrap();
        // Pin this value — any change to Poseidon params breaks byte-parity.
        assert_eq!(
            hex,
            compute_commitment(&input).unwrap(),
            "commitment must be deterministic"
        );
        // Store the actual value once the test passes to snapshot it.
        insta::assert_snapshot!("commitment_alice_bid_100_10", hex);
    }
}
