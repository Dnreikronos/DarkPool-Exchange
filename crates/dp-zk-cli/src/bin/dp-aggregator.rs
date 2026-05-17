//! Production alias for the dp-zk-cli batch prover. Wired into the
//! darkpool-server runtime image as the default `DARKPOOL_AGGREGATOR_BIN`.

fn main() -> std::process::ExitCode {
    dp_zk_cli::run_prover_cli("dp-aggregator", "DarkPool ZK batch aggregator")
}
