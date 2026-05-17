//! Thin process-IO wrappers around [`crate::run_prover_io`]. Split out so
//! the bin shims and stdin/stdout/clap glue can be excluded from coverage
//! analysis without dropping the well-tested core in `lib.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};

use crate::{resolve_keys_dir, run_prover_io, ProverArgs};

/// Stdin JSON → stdout proof bytes. Thin process-IO wrapper over
/// [`run_prover_io`].
///
/// Exit codes:
/// - 0: proof written to stdout.
/// - 2: stdin read failure, missing private_witness, or invalid input.
/// - 3: keys missing / version mismatch.
/// - 4: circuit build, prover, or stdout write failure.
pub fn run_prover(batch_size: usize, keys_dir: Option<PathBuf>) -> ExitCode {
    let dir = resolve_keys_dir(keys_dir);
    run_prover_io(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        batch_size,
        &dir,
    )
}

/// Parse [`ProverArgs`] under a per-binary program name/about string, then
/// dispatch to [`run_prover`]. The shared entrypoint for the `dp-zk-cli` and
/// `dp-aggregator` binaries.
pub fn run_prover_cli(name: &'static str, about: &'static str) -> ExitCode {
    let cmd = ProverArgs::command().name(name).about(about);
    let args = ProverArgs::from_arg_matches(&cmd.get_matches()).unwrap_or_else(|e| e.exit());
    run_prover(args.batch_size, args.proving_key)
}
