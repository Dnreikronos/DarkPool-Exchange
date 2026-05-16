//! Helpers shared by the dp-zk-cli + dp-zk-keygen binaries.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ark_std::rand::rngs::StdRng;
use ark_std::rand::SeedableRng;
use dp_zk::witness::BatchWitness;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

/// Wire-format input matching `SubprocessAggregator`'s extended schema.
#[derive(Debug, Deserialize)]
pub struct AggregatorInput {
    pub batch_id: String,
    pub matches: Vec<AggregatorMatch>,
    pub private_witness: Option<Vec<dp_zk::witness::MatchWitness>>,
    pub policy: Option<dp_zk::witness::Policy>,
}

#[derive(Debug, Deserialize)]
pub struct AggregatorMatch {
    pub auction_id: String,
    pub bid_order_id: String,
    pub ask_order_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
}

/// Resolve the keys directory from CLI flag → env var → manifest default.
pub fn resolve_keys_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(p) = flag {
        return p;
    }
    if let Ok(env) = std::env::var("DARKPOOL_ZK_PROVING_KEY") {
        if !env.is_empty() {
            return PathBuf::from(env);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dp-zk/keys")
}

/// Output of [`build_witness`]: parsed batch metadata, per-match
/// (price, size) pulled directly off the wire, and the full
/// [`BatchWitness`] ready to feed [`dp_zk::BatchProofCircuit`].
#[derive(Debug)]
pub struct ParsedInput {
    pub batch_id: Uuid,
    pub auction_ids: Vec<Uuid>,
    pub witness: BatchWitness,
    pub prices: Vec<Decimal>,
    pub sizes: Vec<Decimal>,
}

/// Build a [`BatchWitness`] from parsed CLI input, falling back to the
/// default policy when omitted. Rejects empty batches and witnesses whose
/// length disagrees with the public matches array — those would either
/// burn a guaranteed-failing prove cycle or silently coerce to a `nil`
/// auction id.
pub fn build_witness(input: AggregatorInput) -> Result<ParsedInput, String> {
    let batch_id = Uuid::parse_str(&input.batch_id).map_err(|e| format!("batch_id: {e}"))?;
    if input.matches.is_empty() {
        return Err("matches array is empty".into());
    }
    let auction_ids: Vec<Uuid> = input
        .matches
        .iter()
        .map(|m| Uuid::parse_str(&m.auction_id).map_err(|e| format!("auction_id: {e}")))
        .collect::<Result<_, _>>()?;
    let auction_id = auction_ids[0];
    if auction_ids.iter().any(|a| *a != auction_id) {
        return Err("matches span multiple auction ids".into());
    }
    let private = input
        .private_witness
        .ok_or_else(|| "missing private_witness; not invoked via dp-zk integration".to_string())?;
    if private.len() != input.matches.len() {
        return Err(format!(
            "matches/private_witness length mismatch: matches={}, private_witness={}",
            input.matches.len(),
            private.len()
        ));
    }
    let prices: Vec<Decimal> = input.matches.iter().map(|m| m.price).collect();
    let sizes: Vec<Decimal> = input.matches.iter().map(|m| m.size).collect();
    let policy = input
        .policy
        .unwrap_or_else(|| dp_zk::witness::DEFAULT_POLICY.into_policy());
    let witness = BatchWitness {
        batch_id,
        auction_id,
        matches: private,
        policy,
    };
    Ok(ParsedInput {
        batch_id,
        auction_ids,
        witness,
        prices,
        sizes,
    })
}

/// Stdin JSON → stdout proof bytes. Shared body of the dp-zk-cli +
/// dp-aggregator binaries.
///
/// Exit codes:
/// - 0: proof written to stdout.
/// - 2: stdin read failure, missing private_witness, or invalid input.
/// - 3: keys missing / version mismatch.
/// - 4: circuit build, prover, or stdout write failure.
pub fn run_prover(batch_size: usize, keys_dir: Option<PathBuf>) -> ExitCode {
    let mut buf = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
        eprintln!("read stdin: {e}");
        return ExitCode::from(2);
    }
    let input: AggregatorInput = match serde_json::from_slice(&buf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("parse input: {e}");
            return ExitCode::from(2);
        }
    };

    let ParsedInput {
        witness,
        prices,
        sizes,
        ..
    } = match build_witness(input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let dir = resolve_keys_dir(keys_dir);
    let meta = match dp_zk::keys::read_metadata(&dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read metadata: {e}");
            return ExitCode::from(3);
        }
    };
    if let Err(e) = meta.check_compatible() {
        eprintln!("{e}");
        return ExitCode::from(3);
    }
    if meta.batch_size as usize != batch_size {
        eprintln!(
            "batch_size mismatch: keys={}, requested={}",
            meta.batch_size, batch_size
        );
        return ExitCode::from(3);
    }
    let pk = match dp_zk::keys::read_pk(&dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("read pk: {e}");
            return ExitCode::from(3);
        }
    };

    let circuit =
        match dp_zk::BatchProofCircuit::from_witness(&witness, &prices, &sizes, batch_size) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("build circuit: {e}");
                return ExitCode::from(4);
            }
        };

    let mut rng = StdRng::from_entropy();
    let proof = match dp_zk::prove(&pk, circuit, &mut rng) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("prove: {e}");
            return ExitCode::from(4);
        }
    };

    if let Err(e) = std::io::stdout().write_all(&proof.0) {
        eprintln!("write stdout: {e}");
        return ExitCode::from(4);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_zk::witness::{MatchWitness, OrderLegWitness};
    use rust_decimal::Decimal;

    fn dummy_leg() -> OrderLegWitness {
        OrderLegWitness {
            trader_id: "aa".repeat(32),
            salt: "bb".repeat(32),
            balance: Decimal::new(1000, 0),
            position: "0".into(),
            limit_price: Decimal::new(100, 0),
            order_size: Decimal::new(10, 0),
            side: 0,
            commitment_key: "ck".into(),
        }
    }

    fn dummy_match_witness() -> MatchWitness {
        MatchWitness {
            bid: dummy_leg(),
            ask: {
                let mut a = dummy_leg();
                a.side = 1;
                a
            },
        }
    }

    fn valid_input() -> AggregatorInput {
        let aid = uuid::Uuid::new_v4().to_string();
        AggregatorInput {
            batch_id: uuid::Uuid::new_v4().to_string(),
            matches: vec![AggregatorMatch {
                auction_id: aid,
                bid_order_id: uuid::Uuid::new_v4().to_string(),
                ask_order_id: uuid::Uuid::new_v4().to_string(),
                price: Decimal::new(100, 0),
                size: Decimal::new(10, 0),
            }],
            private_witness: Some(vec![dummy_match_witness()]),
            policy: None,
        }
    }

    #[test]
    fn build_witness_ok() {
        let result = build_witness(valid_input());
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.prices.len(), 1);
        assert_eq!(parsed.sizes.len(), 1);
    }

    #[test]
    fn build_witness_bad_batch_id() {
        let mut input = valid_input();
        input.batch_id = "not-a-uuid".into();
        let err = build_witness(input).unwrap_err();
        assert!(err.contains("batch_id"));
    }

    #[test]
    fn build_witness_empty_matches() {
        let mut input = valid_input();
        input.matches = vec![];
        let err = build_witness(input).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn build_witness_multiple_auction_ids() {
        let mut input = valid_input();
        let m2 = AggregatorMatch {
            auction_id: uuid::Uuid::new_v4().to_string(),
            bid_order_id: uuid::Uuid::new_v4().to_string(),
            ask_order_id: uuid::Uuid::new_v4().to_string(),
            price: Decimal::new(100, 0),
            size: Decimal::new(5, 0),
        };
        input.matches.push(m2);
        input
            .private_witness
            .as_mut()
            .unwrap()
            .push(dummy_match_witness());
        let err = build_witness(input).unwrap_err();
        assert!(err.contains("multiple auction ids"));
    }

    #[test]
    fn build_witness_missing_private_witness() {
        let mut input = valid_input();
        input.private_witness = None;
        let err = build_witness(input).unwrap_err();
        assert!(err.contains("missing private_witness"));
    }

    #[test]
    fn build_witness_length_mismatch() {
        let mut input = valid_input();
        input.private_witness = Some(vec![dummy_match_witness(), dummy_match_witness()]);
        let err = build_witness(input).unwrap_err();
        assert!(err.contains("mismatch"));
    }

    #[test]
    fn resolve_keys_dir_flag_takes_precedence() {
        let p = resolve_keys_dir(Some(PathBuf::from("/custom/path")));
        assert_eq!(p, PathBuf::from("/custom/path"));
    }

    #[test]
    fn resolve_keys_dir_falls_back_to_manifest() {
        std::env::remove_var("DARKPOOL_ZK_PROVING_KEY");
        let p = resolve_keys_dir(None);
        assert!(p.to_str().unwrap().contains("dp-zk/keys"));
    }
}
