//! Stdin JSON → stdout proof bytes.
//!
//! Exit codes:
//! - 0: proof written to stdout.
//! - 2: missing private_witness or invalid input.
//! - 3: keys missing / version mismatch.
//! - 4: prover error.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ark_std::rand::rngs::StdRng;
use ark_std::rand::SeedableRng;

fn seed_from_time() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEADBEEF)
}
use clap::Parser;
use dp_zk_cli::{build_witness, resolve_keys_dir, AggregatorInput, ParsedInput};

#[derive(Parser, Debug)]
#[command(name = "dp-zk-cli", about = "DarkPool ZK batch prover")]
struct Args {
    /// Directory containing proving_key.bin / verifying_key.bin /
    /// keys_metadata.json.
    #[arg(long, env = "DARKPOOL_ZK_PROVING_KEY")]
    proving_key: Option<PathBuf>,
    /// Override the circuit batch size. Must equal the keygen-time value.
    #[arg(long, env = "DARKPOOL_ZK_BATCH_SIZE", default_value = "8")]
    batch_size: usize,
}

fn main() -> ExitCode {
    let args = Args::parse();

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

    let dir = resolve_keys_dir(args.proving_key);
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
    if meta.batch_size as usize != args.batch_size {
        eprintln!(
            "batch_size mismatch: keys={}, requested={}",
            meta.batch_size, args.batch_size
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

    let circuit = match dp_zk::BatchProofCircuit::from_witness(&witness, &prices, &sizes, args.batch_size) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("build circuit: {e}");
            return ExitCode::from(4);
        }
    };

    let mut rng = StdRng::seed_from_u64(seed_from_time());
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
