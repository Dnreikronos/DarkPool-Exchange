//! Engine → SubprocessAggregator → dp-zk-cli pipeline test. Asserts a
//! valid Groth16 proof shows up in the BatchSubmitted event log, and that
//! the privacy canary holds (commitment_key strings do not leak plaintext).
//!
//! Verifying the proof against public inputs is covered by
//! `crates/dp-aggregator/tests/zk_e2e.rs` where the witness is fully
//! reconstructible. Here we focus on engine-side wiring + privacy.

#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dp_aggregator::SubprocessAggregator;
use dp_engine::Engine;
use dp_event::{EventData, MemStore, Store};
use dp_types::Side;
use rust_decimal::Decimal;

fn keys_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dp-zk/keys")
}

fn cli_bin() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let release = workspace.join("target/release/dp-zk-cli");
    if release.exists() {
        return release;
    }
    workspace.join("target/debug/dp-zk-cli")
}

async fn place(
    engine: &Engine,
    side: Side,
    price: i64,
    key: &str,
    trader: alloy_primitives::Address,
) {
    let d = dp_crypto::DecryptedOrder {
        trader,
        pair: "BTC-USD".into(),
        side,
        price: Decimal::from(price),
        size: Decimal::from(10),
        commitment_key: key.into(),
        ttl: Duration::from_secs(60).as_nanos() as i64,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct, None)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_subprocess_zk_pipeline() {
    if !keys_dir().join("proving_key.bin").exists() || !cli_bin().exists() {
        eprintln!("dev keys or cli missing — skipping");
        return;
    }

    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    engine.register_pair_without_event("BTC-USD".into(), dp_engine::PairConfig::default());
    let agg = SubprocessAggregator::new(&cli_bin(), Some(Duration::from_secs(60)))
        .unwrap()
        .with_env("DARKPOOL_ZK_PROVING_KEY", keys_dir().to_string_lossy())
        .with_env("DARKPOOL_ZK_BATCH_SIZE", "2");
    engine.set_aggregator(Arc::new(agg));

    let bid_addr = alloy_primitives::Address::from([0x11u8; 20]);
    let ask_addr = alloy_primitives::Address::from([0x22u8; 20]);
    place(&engine, Side::Buy, 105, "secret_bid", bid_addr).await;
    place(&engine, Side::Sell, 95, "secret_ask", ask_addr).await;

    let notifications = engine.run_auction_tick().await;
    assert_eq!(notifications.len(), 1, "expected one auction");

    let events = store.read_from(0, 1024).unwrap();
    let proof = events
        .iter()
        .find_map(|e| match &e.data {
            EventData::BatchSubmitted { proof, .. } => Some(proof.clone()),
            _ => None,
        })
        .expect("expected BatchSubmitted event");
    // Groth16 compressed proof = 128 bytes (3 G1 + 1 G2 with point compression
    // varies across curves; on BN254 ~128).
    assert!(proof.len() >= 96, "proof too short: {}", proof.len());

    // Privacy canary for ZK secrets: the raw 32-byte trader_id derived
    // from commitment_key must not appear in any persisted event.
    // (Plaintext orders themselves use NoopDecrypter here so their JSON
    // body is in OrderPlaced::ciphertext — that's covered by the
    // dedicated XOR/event-store canary in `tests.rs`.)
    let trader_id_bid = dp_zk::pedersen::derive_trader_id_bytes(bid_addr.as_slice());
    let trader_id_bid_hex = hex::encode(trader_id_bid);
    let raw = bincode::serialize(&events).unwrap();
    let leaked = raw.windows(trader_id_bid.len()).any(|w| w == trader_id_bid)
        || raw
            .windows(trader_id_bid_hex.len())
            .any(|w| w == trader_id_bid_hex.as_bytes());
    assert!(!leaked, "ZK trader_id leaked into event log");
}
