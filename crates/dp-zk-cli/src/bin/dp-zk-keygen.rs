//! One-shot trusted setup. Writes proving_key.bin, verifying_key.bin,
//! keys_metadata.json into the requested directory.
//!
//! WARNING: development-only single-machine setup. Production deployments
//! must run a multi-party Powers-of-Tau + Phase 2 ceremony.

use std::path::PathBuf;
use std::process::ExitCode;

use ark_std::rand::rngs::StdRng;
use ark_std::rand::SeedableRng;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "dp-zk-keygen", about = "DarkPool ZK keygen (dev only)")]
struct Args {
    #[arg(long, default_value = "8")]
    batch_size: usize,
    #[arg(long)]
    out: PathBuf,
    /// Deterministic seed (dev only). Omit for entropy.
    #[arg(long)]
    seed: Option<u64>,
}

fn seed_from_time() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEADBEEF)
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut rng = match args.seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::seed_from_u64(seed_from_time()),
    };

    eprintln!(
        "generating Groth16 keys for batch_size={} into {} ...",
        args.batch_size,
        args.out.display()
    );
    let (pk, vk) = match dp_zk::circuit::setup(args.batch_size, &mut rng) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("setup: {e}");
            return ExitCode::from(2);
        }
    };
    let meta = match dp_zk::keys::write_keys_to_dir(&args.out, &pk, &vk, args.batch_size as u32) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("write keys: {e}");
            return ExitCode::from(2);
        }
    };
    eprintln!(
        "wrote keys (pk_sha256={}, vk_sha256={})",
        &meta.proving_key_sha256[..16],
        &meta.verifying_key_sha256[..16]
    );
    ExitCode::SUCCESS
}
