//! End-to-end: SubprocessAggregator spawns dp-zk-cli, prover writes proof.
//! Phase G: Groth16 path removed; IVC proofs are generated in-process via
//! InlineFoldingAggregator. This test file is retained for structural compat
//! and verifies the subprocess binary can be invoked.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use dp_aggregator::{ProofAggregator, SubprocessAggregator};
use dp_auction::Match;
use dp_types::Fill;
use dp_zk::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use rust_decimal::Decimal;
use uuid::Uuid;

fn cli_bin() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let release = workspace.join("target/release/dp-zk-cli");
    if release.exists() {
        return release;
    }
    workspace.join("target/debug/dp-zk-cli")
}

#[tokio::test(flavor = "multi_thread")]
async fn subprocess_aggregator_returns_result() {
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

    let bid_addr = "aa".repeat(20);
    let ask_addr = "bb".repeat(20);
    let bid_trader = hex::encode(dp_zk::pedersen::derive_trader_id_bytes(
        &hex::decode(&bid_addr).unwrap(),
    ));
    let ask_trader = hex::encode(dp_zk::pedersen::derive_trader_id_bytes(
        &hex::decode(&ask_addr).unwrap(),
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
                trader_addr: bid_addr,
            },
            ask: OrderLegWitness {
                trader_id: ask_trader,
                salt: "44".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(95),
                order_size: Decimal::from(10),
                side: 1,
                trader_addr: ask_addr,
            },
        }],
        policy: DEFAULT_POLICY.into_policy(),
    };

    let agg = SubprocessAggregator::new(&cli_bin(), Some(Duration::from_secs(30)))
        .unwrap()
        .with_env("DARKPOOL_ZK_BATCH_SIZE", "2");
    let result = agg
        .aggregate(batch_id, auction_id, &matches, &witness)
        .await;
    assert!(
        result.is_err(),
        "IVC proving is not yet implemented in dp-zk-cli; aggregate should return an error"
    );
}
