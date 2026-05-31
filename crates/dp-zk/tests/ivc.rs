//! Integration tests for the HyperNova IVC chain.
//!
//! The folding tests (`fold_*`) do full IVC prove+verify which requires
//! HyperNova setup (~seconds on fast hardware). They are marked `#[ignore]`
//! so the normal `cargo test` suite stays fast. Run them explicitly with:
//!
//! ```text
//! cargo test --release -p dp-zk -- ivc --ignored
//! ```
//!
//! The `restart_from_fresh_accumulator` test does NOT fold so it is fast
//! and NOT ignored.

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
use ark_crypto_primitives::sponge::CryptographicSponge;
use ark_ff::Zero;
use ark_std::rand::SeedableRng;
use dp_zk::{
    folding::{compress_and_finalize, fold_step, generate_params, init_accumulator, verify_final},
    pedersen::{derive_trader_id, poseidon_config},
    step_circuit::AuctionExternalInputs,
    witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY},
};
use rust_decimal::Decimal;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_rng() -> ark_std::rand::rngs::StdRng {
    ark_std::rand::rngs::StdRng::seed_from_u64(42)
}

fn trader_id_hex(addr_hex: &str) -> String {
    use ark_ff::{BigInteger, PrimeField};
    let addr = hex::decode(addr_hex.trim_start_matches("0x")).unwrap();
    let f = derive_trader_id(&addr).unwrap();
    let mut bytes = f.into_bigint().to_bytes_be();
    while bytes.len() < 32 {
        bytes.insert(0, 0);
    }
    hex::encode(bytes)
}

fn sample_witness() -> (BatchWitness, Vec<Decimal>, Vec<Decimal>) {
    let bid_addr = "aa".repeat(20);
    let ask_addr = "bb".repeat(20);
    let m = MatchWitness {
        bid: OrderLegWitness {
            trader_id: trader_id_hex(&bid_addr),
            salt: "22".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(105),
            order_size: Decimal::from(10),
            side: 0,
            trader_addr: bid_addr,
        },
        ask: OrderLegWitness {
            trader_id: trader_id_hex(&ask_addr),
            salt: "44".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(95),
            order_size: Decimal::from(10),
            side: 1,
            trader_addr: ask_addr,
        },
    };
    let w = BatchWitness {
        batch_id: Uuid::nil(),
        auction_id: Uuid::nil(),
        matches: vec![m],
        policy: DEFAULT_POLICY.into_policy(),
    };
    (w, vec![Decimal::from(100)], vec![Decimal::from(10)])
}

fn sample_ext(batch_size: usize) -> AuctionExternalInputs {
    let (w, prices, sizes) = sample_witness();
    AuctionExternalInputs::from_witness(&w, &prices, &sizes, batch_size)
        .expect("sample_ext: from_witness")
}

/// Compute `z_0 = [0, 0, policy_hash, 0, 0]` where
/// `policy_hash = poseidon(min_size, min_price, position_limit)`; the two
/// trailing slots are the empty settlement hash-chain (#153) and admitted-set
/// chain (#157) accumulators.
fn z_0_for(ext: &AuctionExternalInputs) -> [Fr; 5] {
    let cfg = poseidon_config();
    let mut s = PoseidonSponge::<Fr>::new(&cfg);
    s.absorb(&vec![ext.min_size, ext.min_price, ext.position_limit]);
    let policy_hash = s.squeeze_field_elements::<Fr>(1)[0];
    [Fr::zero(), Fr::zero(), policy_hash, Fr::zero(), Fr::zero()]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn restart_from_fresh_accumulator() {
    // This test does NOT fold — it only checks that init_accumulator returns
    // a clean accumulator. Should complete in <1s.
    let mut rng = make_rng();
    let batch_size = 2;
    let ext = sample_ext(batch_size);
    let z_0 = z_0_for(&ext);
    let params = generate_params(batch_size, &mut rng).expect("generate_params");
    let acc = init_accumulator(&params, batch_size, z_0).expect("init_accumulator");
    assert_eq!(acc.n_steps, 0);
    assert_eq!(acc.z_0, z_0);
}

#[test]
#[ignore]
fn fold_single_step_verifies() {
    let mut rng = make_rng();
    let batch_size = 2;
    let ext = sample_ext(batch_size);
    let z_0 = z_0_for(&ext);
    let params = generate_params(batch_size, &mut rng).expect("generate_params");
    let mut acc = init_accumulator(&params, batch_size, z_0).expect("init_accumulator");
    fold_step(&mut acc, ext, &mut rng).expect("fold_step");
    let proof = compress_and_finalize(&acc).expect("compress_and_finalize");
    verify_final(&params, &proof).expect("verify_final");

    assert_eq!(proof.n_steps, 1);
    assert_eq!(proof.z_0, z_0);
    // state_hash should have changed from the initial zero
    assert_ne!(
        proof.z_n[0],
        Fr::zero(),
        "state_hash must change after folding"
    );
    // round_nonce should be 1
    assert_eq!(
        proof.z_n[1],
        Fr::from(1u64),
        "round_nonce must be 1 after one step"
    );
    // policy_hash must be invariant
    assert_eq!(proof.z_n[2], z_0[2], "policy_hash must remain invariant");
    // settlement_acc must survive the end-to-end fold/finalize path: folding
    // one active match advances the hash-chain off its zero seed.
    assert_ne!(
        proof.z_n[3],
        Fr::zero(),
        "settlement_acc must change after folding an active match"
    );
    // admit_chain (#157) folds the round's admitted-set root, so it too leaves
    // the zero seed after a real round.
    assert_ne!(
        proof.z_n[4],
        Fr::zero(),
        "admit_chain must change after folding a round with an admitted set"
    );
}

#[test]
#[ignore]
fn fold_n_steps_correct_state() {
    let n: u64 = 3;
    let mut rng = make_rng();
    let batch_size = 2;
    let ext = sample_ext(batch_size);
    let z_0 = z_0_for(&ext);
    let params = generate_params(batch_size, &mut rng).expect("generate_params");
    let mut acc = init_accumulator(&params, batch_size, z_0).expect("init_accumulator");
    for _ in 0..n {
        fold_step(&mut acc, sample_ext(batch_size), &mut rng).expect("fold_step");
    }
    assert_eq!(acc.n_steps, n);
    let proof = compress_and_finalize(&acc).expect("compress_and_finalize");
    // round_nonce == number of folds performed
    assert_eq!(
        proof.z_n[1],
        Fr::from(n),
        "round_nonce must equal step count"
    );
}

#[test]
#[ignore]
fn compress_after_n_folds_verifies() {
    let n: u64 = 2;
    let mut rng = make_rng();
    let batch_size = 2;
    let ext = sample_ext(batch_size);
    let z_0 = z_0_for(&ext);
    let params = generate_params(batch_size, &mut rng).expect("generate_params");
    let mut acc = init_accumulator(&params, batch_size, z_0).expect("init_accumulator");
    for _ in 0..n {
        fold_step(&mut acc, sample_ext(batch_size), &mut rng).expect("fold_step");
    }
    let proof = compress_and_finalize(&acc).expect("compress_and_finalize");
    verify_final(&params, &proof).expect("verify_final");
}

/// Slow acceptance test: 60 auction rounds. Only run in CI with explicit
/// `--ignored` and `--release` flags.
#[test]
#[ignore]
fn fold_sixty_steps_acceptance() {
    let n: u64 = 60;
    let mut rng = make_rng();
    let batch_size = 2;
    let ext = sample_ext(batch_size);
    let z_0 = z_0_for(&ext);
    let params = generate_params(batch_size, &mut rng).expect("generate_params");
    let mut acc = init_accumulator(&params, batch_size, z_0).expect("init_accumulator");
    for _ in 0..n {
        fold_step(&mut acc, sample_ext(batch_size), &mut rng).expect("fold_step");
    }
    assert_eq!(acc.n_steps, n);
    let proof = compress_and_finalize(&acc).expect("compress_and_finalize");
    verify_final(&params, &proof).expect("verify_final");
    assert_eq!(
        proof.z_n[1],
        Fr::from(n),
        "round_nonce must equal 60 after 60 steps"
    );
    assert_eq!(proof.z_n[2], z_0[2], "policy_hash invariant after 60 steps");
}
