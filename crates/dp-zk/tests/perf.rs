//! Long-running prove-time smoke. Ignored by default; run with
//!
//! ```text
//! cargo test --release -p dp-zk -- --ignored
//! ```
//!
//! Override the wall-clock budget via `DARKPOOL_ZK_PERF_LIMIT` (seconds).

use std::time::Instant;

use ark_std::rand::rngs::StdRng;
use ark_std::rand::SeedableRng;
use dp_zk::pedersen::derive_trader_id_bytes;
use dp_zk::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use dp_zk::{prove, BatchProofCircuit};
use rust_decimal::Decimal;
use uuid::Uuid;

#[test]
#[ignore]
fn prove_batch_within_budget() {
    const BATCH_SIZE: usize = 256;

    let limit_secs: u64 = std::env::var("DARKPOOL_ZK_PERF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let mut rng = StdRng::seed_from_u64(7);

    let setup_start = Instant::now();
    let (pk, _vk) = dp_zk::circuit::setup(BATCH_SIZE, &mut rng).expect("setup");
    eprintln!("setup: {:?}", setup_start.elapsed());

    let mut matches = Vec::with_capacity(BATCH_SIZE);
    let mut prices = Vec::with_capacity(BATCH_SIZE);
    let mut sizes = Vec::with_capacity(BATCH_SIZE);
    for i in 0..BATCH_SIZE {
        let bid_key = format!("bid_{i}");
        let ask_key = format!("ask_{i}");
        let bid_trader = hex::encode(derive_trader_id_bytes(bid_key.as_bytes()));
        let ask_trader = hex::encode(derive_trader_id_bytes(ask_key.as_bytes()));
        matches.push(MatchWitness {
            bid: OrderLegWitness {
                trader_id: bid_trader,
                salt: format!("{:064x}", i as u64 * 2),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(105),
                side: 0,
                commitment_key: bid_key,
            },
            ask: OrderLegWitness {
                trader_id: ask_trader,
                salt: format!("{:064x}", i as u64 * 2 + 1),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(95),
                side: 1,
                commitment_key: ask_key,
            },
        });
        prices.push(Decimal::from(100));
        sizes.push(Decimal::from(10));
    }
    let witness = BatchWitness {
        batch_id: Uuid::new_v4(),
        auction_id: Uuid::new_v4(),
        matches,
        policy: DEFAULT_POLICY.into_policy(),
    };

    let circuit =
        BatchProofCircuit::from_witness(&witness, &prices, &sizes, BATCH_SIZE).expect("circuit");

    let prove_start = Instant::now();
    let _proof = prove(&pk, circuit, &mut rng).expect("prove");
    let elapsed = prove_start.elapsed();
    eprintln!(
        "proved batch_size={BATCH_SIZE} in {:?} (budget {}s)",
        elapsed, limit_secs
    );
    assert!(
        elapsed.as_secs_f64() <= limit_secs as f64,
        "prove took {:?}, exceeds {}s budget",
        elapsed,
        limit_secs
    );
}
