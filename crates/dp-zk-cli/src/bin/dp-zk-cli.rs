//! DarkPool ZK CLI — subcommand-based interface.
//!
//! Subcommands:
//! - `commit`: compute Poseidon commitment for a single order.
//! - `prove-single-order`: Groth16 proof of commitment preimage.
//! - `prove-batch`: legacy stdin→stdout batch prover (same as dp-aggregator).

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dp_zk_cli::commit::{run_commit, CommitArgs};
use dp_zk_cli::prove_single::{run_prove_single, ProveSingleArgs};
use dp_zk_cli::ProverArgs;

#[derive(Parser)]
#[command(name = "dp-zk-cli", about = "DarkPool ZK toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compute Poseidon commitment for a single order (JSON → hex).
    Commit(CommitArgs),
    /// Generate Groth16 proof of commitment preimage knowledge.
    ProveSingleOrder(ProveSingleArgs),
    /// Legacy batch prover (stdin JSON → stdout proof bytes).
    ProveBatch(ProverArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Commit(args) => run_commit(args),
        Commands::ProveSingleOrder(args) => run_prove_single(args),
        Commands::ProveBatch(args) => {
            dp_zk_cli::run_prover(args.batch_size, args.proving_key)
        }
    }
}
