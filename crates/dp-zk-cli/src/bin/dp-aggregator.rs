//! Production alias for the dp-zk-cli batch prover. Wired into the
//! darkpool-server runtime image as the default `DARKPOOL_AGGREGATOR_BIN`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use dp_zk_cli::run_prover;

#[derive(Parser, Debug)]
#[command(name = "dp-aggregator", about = "DarkPool ZK batch aggregator")]
struct Args {
    #[arg(long, env = "DARKPOOL_ZK_PROVING_KEY")]
    proving_key: Option<PathBuf>,
    #[arg(long, env = "DARKPOOL_ZK_BATCH_SIZE", default_value = "8")]
    batch_size: usize,
}

fn main() -> ExitCode {
    let args = Args::parse();
    run_prover(args.batch_size, args.proving_key)
}
