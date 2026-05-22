//! End-to-end: SubprocessAggregator spawns dp-zk-cli, prover writes proof,
//! we verify it with dp-zk's verifier.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use dp_aggregator::{ProofAggregator, SubprocessAggregator};
use dp_auction::Match;
use dp_types::Fill;
use dp_zk::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use rust_decimal::Decimal;
use uuid::Uuid;

fn keys_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dp-zk/keys")
}

fn cli_bin() -> PathBuf {
    // CARGO_BIN_EXE_* is only auto-set for binaries in the same crate, so
    // we locate the binary from the workspace target dir. Prefer release
    // (the debug prover is too slow for default test timeouts).
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let release = workspace.join("target/release/dp-zk-cli");
    if release.exists() {
        return release;
    }
    workspace.join("target/debug/dp-zk-cli")
}

#[tokio::test(flavor = "multi_thread")]
async fn subprocess_zk_round_trip() {
    if !keys_dir().join("proving_key.bin").exists() {
        eprintln!("dev keys missing — run dp-zk-keygen first; skipping");
        return;
    }
    if !cli_bin().exists() {
        eprintln!("dp-zk-cli not built — run `cargo build -p dp-zk-cli` first; skipping");
        return;
    }
    let bid_id = Uuid::new_v4();
    let ask_id = Uuid::new_v4();
    let auction_id = Uuid::new_v4();
    let batch_id = Uuid::new_v4();

    let matches = vec![Match {
        bid: Fill {
            order_id: bid_id,
            size: Decimal::from(10),
        },
        ask: Fill {
            order_id: ask_id,
            size: Decimal::from(10),
        },
        price: Decimal::from(100),
        size: Decimal::from(10),
    }];

    let bid_key = "bid_key".to_string();
    let ask_key = "ask_key".to_string();
    let bid_trader = hex::encode(dp_zk::commitment::derive_trader_id_bytes(
        bid_key.as_bytes(),
    ));
    let ask_trader = hex::encode(dp_zk::commitment::derive_trader_id_bytes(
        ask_key.as_bytes(),
    ));
    let witness = BatchWitness {
        batch_id,
        auction_id,
        matches: vec![MatchWitness {
            bid: OrderLegWitness {
                trader_id: bid_trader,
                salt: "22".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(105),
                order_size: Decimal::from(10),
                side: 0,
                commitment_key: bid_key,
            },
            ask: OrderLegWitness {
                trader_id: ask_trader,
                salt: "44".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(95),
                order_size: Decimal::from(10),
                side: 1,
                commitment_key: ask_key,
            },
        }],
        policy: DEFAULT_POLICY.into_policy(),
    };

    // Diagnostic: invoke binary directly via tokio with the same payload
    // the SubprocessAggregator would send. Verifies wire format + runtime.
    let agg = SubprocessAggregator::new(&cli_bin(), Some(Duration::from_secs(30)))
        .unwrap()
        .with_env("DARKPOOL_ZK_PROVING_KEY", keys_dir().to_string_lossy())
        .with_env("DARKPOOL_ZK_BATCH_SIZE", "2");
    let result = agg
        .aggregate(batch_id, auction_id, &matches, &witness)
        .await;
    let proof = match result {
        Ok(p) => p,
        Err(e) => panic!("aggregator error: {e:?}"),
    };
    assert!(!proof.is_empty(), "expected non-empty proof");

    // Verify.
    let circuit = dp_zk::BatchProofCircuit::from_witness(
        &witness,
        &[Decimal::from(100)],
        &[Decimal::from(10)],
        2,
    )
    .unwrap();
    let public_inputs = circuit.public_inputs();
    let vk = dp_zk::keys::read_vk(&keys_dir()).unwrap();
    let ok = dp_zk::verify(&vk, &public_inputs, &proof).unwrap();
    assert!(ok, "proof did not verify");
}
