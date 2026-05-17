//! Stdin JSON → stdout proof bytes.
//!
//! Exit codes:
//! - 0: proof written to stdout.
//! - 2: stdin read failure, missing private_witness, or invalid input.
//! - 3: keys missing / version mismatch.
//! - 4: circuit build, prover, or stdout write failure.

fn main() -> std::process::ExitCode {
    dp_zk_cli::run_prover_cli("dp-zk-cli", "DarkPool ZK batch prover")
}
