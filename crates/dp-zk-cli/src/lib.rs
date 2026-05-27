//! Helpers shared by the dp-zk-cli + dp-aggregator binaries.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use dp_zk::witness::BatchWitness;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

/// CLI flags shared by every prover entrypoint (`dp-zk-cli`, `dp-aggregator`).
#[derive(Parser, Debug)]
pub struct ProverArgs {
    /// Directory containing params (currently unused; kept for CLI compat).
    #[arg(long, env = "DARKPOOL_ZK_PROVING_KEY")]
    pub proving_key: Option<PathBuf>,
    /// Override the circuit batch size.
    #[arg(long, env = "DARKPOOL_ZK_BATCH_SIZE", default_value = "8")]
    pub batch_size: usize,
}

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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dp-zk/params")
}

/// Output of [`build_witness`]: parsed batch metadata, per-match
/// (price, size) pulled directly off the wire, and the full
/// [`BatchWitness`] ready to feed the IVC prover.
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
/// length disagrees with the public matches array.
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

/// Failure surface for [`prove_from_bytes`].
#[derive(Debug)]
pub struct ProveError {
    pub exit_code: u8,
    pub message: String,
}

impl ProveError {
    fn new(exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
}

/// Parse JSON input and validate the witness. This is a stub entrypoint for
/// the IVC path: the subprocess aggregator calling this binary submits
/// witness JSON, and the binary returns proof bytes. In Phase G the IVC
/// proof generation happens in-process via `InlineFoldingAggregator`; this
/// binary is retained for CLI compat and future use.
///
/// `batch_size` and `params_dir` are accepted for CLI signature parity with
/// the legacy Groth16 prover but are unused on the IVC stub path.
///
/// Exit codes:
/// - 2: JSON parse failure or invalid witness.
/// - 3: params missing or incompatible.
/// - 4: prover failure.
pub fn prove_from_bytes(
    input_bytes: &[u8],
    _batch_size: usize,
    _params_dir: &Path,
) -> Result<Vec<u8>, ProveError> {
    let input: AggregatorInput = serde_json::from_slice(input_bytes)
        .map_err(|e| ProveError::new(2, format!("parse input: {e}")))?;
    let _parsed = build_witness(input).map_err(|e| ProveError::new(2, e))?;
    Err(ProveError::new(
        4,
        "IVC proving is not implemented in dp-zk-cli; use InlineFoldingAggregator",
    ))
}

/// Generic-IO core of [`run_prover`]: reads JSON from `stdin`, dispatches to
/// [`prove_from_bytes`], writes the resulting proof bytes to `stdout`, and
/// maps every failure to an exit code via [`ProveError`].
pub fn run_prover_io<R: Read, W: Write>(
    mut stdin: R,
    mut stdout: W,
    batch_size: usize,
    params_dir: &Path,
) -> ExitCode {
    let mut buf = Vec::new();
    if let Err(e) = stdin.read_to_end(&mut buf) {
        eprintln!("read stdin: {e}");
        return ExitCode::from(2);
    }
    match prove_from_bytes(&buf, batch_size, params_dir) {
        Ok(bytes) => {
            if let Err(e) = stdout.write_all(&bytes) {
                eprintln!("write stdout: {e}");
                return ExitCode::from(4);
            }
            ExitCode::SUCCESS
        }
        Err(ProveError { exit_code, message }) => {
            eprintln!("{message}");
            ExitCode::from(exit_code)
        }
    }
}

pub mod cli;
pub mod commit;
pub mod prove_single;
pub use cli::{run_prover, run_prover_cli};

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

    /// Serializes the env-var tests below. cargo runs tests in parallel and
    /// `set_var`/`remove_var` are global, so without this guard one test's
    /// teardown races another's setup.
    fn env_var_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn resolve_keys_dir_falls_back_to_manifest() {
        let _g = env_var_lock();
        std::env::remove_var("DARKPOOL_ZK_PROVING_KEY");
        let p = resolve_keys_dir(None);
        assert!(p.to_str().unwrap().contains("dp-zk/params"));
    }

    fn valid_input_bytes() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "batch_id": uuid::Uuid::new_v4().to_string(),
            "matches": [{
                "auction_id": uuid::Uuid::new_v4().to_string(),
                "bid_order_id": uuid::Uuid::new_v4().to_string(),
                "ask_order_id": uuid::Uuid::new_v4().to_string(),
                "price": "100",
                "size": "10",
            }],
            "private_witness": serde_json::to_value(vec![dummy_match_witness()]).unwrap(),
        }))
        .unwrap()
    }

    #[test]
    fn prove_from_bytes_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        let err = prove_from_bytes(b"not json", 8, dir.path()).unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("parse input"));
    }

    #[test]
    fn prove_from_bytes_invalid_witness_returns_code_2() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "batch_id": "not-a-uuid",
            "matches": [],
            "private_witness": null,
        }))
        .unwrap();
        let err = prove_from_bytes(&bytes, 8, dir.path()).unwrap_err();
        assert_eq!(err.exit_code, 2);
    }

    #[test]
    fn prove_from_bytes_returns_unimplemented_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = prove_from_bytes(&valid_input_bytes(), 8, dir.path()).unwrap_err();
        assert_eq!(err.exit_code, 4);
    }

    #[test]
    fn run_prover_io_bad_json_returns_2() {
        let dir = tempfile::tempdir().unwrap();
        let stdin = std::io::Cursor::new(b"not json".to_vec());
        let mut stdout: Vec<u8> = Vec::new();
        let code = run_prover_io(stdin, &mut stdout, 8, dir.path());
        assert_eq!(code, ExitCode::from(2));
        assert!(stdout.is_empty());
    }

    #[test]
    fn run_prover_io_stdin_read_error_returns_2() {
        struct Boom;
        impl std::io::Read for Boom {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let mut stdout: Vec<u8> = Vec::new();
        let code = run_prover_io(Boom, &mut stdout, 8, dir.path());
        assert_eq!(code, ExitCode::from(2));
    }
}
