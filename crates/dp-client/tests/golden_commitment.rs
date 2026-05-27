use dp_client::commitment::{
    bytes_to_scalar, commit_native, derive_trader_id, scalar_to_be_bytes, OrderCommitmentInput,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct Vector {
    label: String,
    commitment_key: String,
    side: u8,
    price: String,
    size: String,
    salt_hex: String,
    expected_commitment_hex: String,
    expected_trader_id_hex: String,
}

fn load_vectors() -> Vec<Vector> {
    let json = include_str!("fixtures/commitment_vectors.json");
    serde_json::from_str(json).expect("invalid fixture JSON")
}

#[test]
fn golden_commitments_match_dp_zk_cli() {
    let vectors = load_vectors();
    assert!(vectors.len() >= 10, "need at least 10 golden vectors");

    for v in &vectors {
        let trader_id = derive_trader_id(v.commitment_key.as_bytes());
        let price = Decimal::from_str(&v.price).unwrap();
        let size = Decimal::from_str(&v.size).unwrap();
        let salt_bytes = hex::decode(&v.salt_hex).unwrap();
        assert_eq!(salt_bytes.len(), 32, "{}: salt must be 32 bytes", v.label);
        let salt = bytes_to_scalar(&salt_bytes);

        let input =
            OrderCommitmentInput::from_decimals(trader_id, v.side, price, size, salt).unwrap();
        let commitment = commit_native(&input);
        let commitment_hex = hex::encode(scalar_to_be_bytes(commitment));

        assert_eq!(
            commitment_hex, v.expected_commitment_hex,
            "{}: commitment mismatch",
            v.label
        );
    }
}

#[test]
fn golden_trader_ids_match_dp_zk_cli() {
    let vectors = load_vectors();

    for v in &vectors {
        let trader_id = derive_trader_id(v.commitment_key.as_bytes());
        let trader_id_hex = hex::encode(scalar_to_be_bytes(trader_id));

        assert_eq!(
            trader_id_hex, v.expected_trader_id_hex,
            "{}: trader_id mismatch",
            v.label
        );
    }
}
