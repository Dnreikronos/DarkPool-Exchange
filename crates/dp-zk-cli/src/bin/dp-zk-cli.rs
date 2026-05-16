//! Stdin JSON → stdout proof bytes.
//!
//! Exit codes:
//! - 0: proof written to stdout.
//! - 2: stdin read failure, missing private_witness, or invalid input.
//! - 3: keys missing / version mismatch.
//! - 4: circuit build, prover, or stdout write failure.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use dp_zk_cli::run_prover;

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
    run_prover(args.batch_size, args.proving_key)
}
