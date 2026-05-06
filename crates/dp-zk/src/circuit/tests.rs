use super::*;
use crate::pedersen::derive_trader_id;
use crate::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use ark_ff::PrimeField;
use ark_ff::BigInteger;
use ark_relations::r1cs::{ConstraintSystem, ConstraintSynthesizer};
use ark_std::rand::SeedableRng;
use ark_std::rand::rngs::StdRng;
use rust_decimal::Decimal;
use uuid::Uuid;

fn trader_id_hex(commitment_key: &str) -> String {
    let f = derive_trader_id(commitment_key.as_bytes()).unwrap();
    let mut bytes = f.into_bigint().to_bytes_be();
    while bytes.len() < 32 {
        bytes.insert(0, 0);
    }
    hex::encode(bytes)
}

fn sample_witness() -> (BatchWitness, Vec<Decimal>, Vec<Decimal>) {
    let bid_key = "bid_key".to_string();
    let ask_key = "ask_key".to_string();
    let m = MatchWitness {
        bid: OrderLegWitness {
            trader_id: trader_id_hex(&bid_key),
            salt: "22".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(105),
            order_size: Decimal::from(10),
            side: 0,
            commitment_key: bid_key,
        },
        ask: OrderLegWitness {
            trader_id: trader_id_hex(&ask_key),
            salt: "44".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(95),
            order_size: Decimal::from(10),
            side: 1,
            commitment_key: ask_key,
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

#[test]
fn satisfied_with_valid_witness() {
    let (w, prices, sizes) = sample_witness();
    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(cs.is_satisfied().unwrap());
}

#[test]
fn rejects_same_side() {
    let (mut w, prices, sizes) = sample_witness();
    w.matches[0].ask.side = 0;
    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(!cs.is_satisfied().unwrap());
}

#[test]
fn rejects_match_price_above_bid_limit() {
    let (mut w, _, sizes) = sample_witness();
    // Bid limit is 105, we set match price = 110 -> should fail crossing.
    w.matches[0].bid.limit_price = Decimal::from(105);
    let circuit = BatchProofCircuit::from_witness(&w, &[Decimal::from(110)], &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(!cs.is_satisfied().unwrap());
}

#[test]
fn rejects_insufficient_balance() {
    let (mut w, prices, sizes) = sample_witness();
    // Notional = 100 * 10 = 1000 USD. Drop balance below that.
    w.matches[0].bid.balance = Decimal::from(500);
    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(!cs.is_satisfied().unwrap());
}

#[test]
fn rejects_position_breach() {
    let (mut w, prices, sizes) = sample_witness();
    // policy.position_limit = 2^58. Setting position close to the limit
    // such that position + size overflows it.
    let limit: i128 = 1i128 << 58;
    w.matches[0].bid.position = limit.to_string();
    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(!cs.is_satisfied().unwrap());
}

#[test]
fn rejects_forged_trader_id() {
    let (mut w, prices, sizes) = sample_witness();
    // trader_id no longer matches poseidon(commitment_key).
    w.matches[0].bid.trader_id = "ab".repeat(32);
    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let cs = ConstraintSystem::<Fr>::new_ref();
    circuit.generate_constraints(cs.clone()).unwrap();
    assert!(!cs.is_satisfied().unwrap());
}

#[test]
fn proves_and_verifies_round_trip() {
    let mut rng = StdRng::seed_from_u64(42);
    let (w, prices, sizes) = sample_witness();
    let (pk, vk) = setup(2, &mut rng).unwrap();

    let circuit = BatchProofCircuit::from_witness(&w, &prices, &sizes, 2).unwrap();
    let public_inputs = circuit.public_inputs();
    let proof = prove(&pk, circuit, &mut rng).unwrap();
    assert!(verify(&vk, &public_inputs, &proof.0).unwrap());
}
