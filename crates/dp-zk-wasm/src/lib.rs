use ark_bn254::Fr;
use ark_serialize::{CanonicalSerialize, Compress};
use serde::Deserialize;

use dp_zk::commitment_circuit::{setup_and_prove, CommitmentPreimageCircuit};
use dp_zk::encoding::decimal_to_scalar;
use dp_zk::pedersen::{bytes_to_scalar, derive_trader_id};

#[derive(Deserialize)]
struct WitnessInput {
    commitment_key: String,
    side: u8,
    price: String,
    size: String,
    salt_hex: String,
}

pub fn prove_order(witness_json: &str) -> Result<ProveResult, String> {
    let w: WitnessInput =
        serde_json::from_str(witness_json).map_err(|e| format!("invalid witness JSON: {e}"))?;

    let commitment_key_bytes =
        hex::decode(&w.commitment_key).map_err(|e| format!("bad commitment_key hex: {e}"))?;
    let trader_id =
        derive_trader_id(&commitment_key_bytes).map_err(|e| format!("derive_trader_id: {e}"))?;

    if w.side > 1 {
        return Err(format!("side must be 0 or 1, got {}", w.side));
    }
    let side_fr = Fr::from(w.side as u64);

    let price: rust_decimal::Decimal = w.price.parse().map_err(|e| format!("bad price: {e}"))?;
    let size: rust_decimal::Decimal = w.size.parse().map_err(|e| format!("bad size: {e}"))?;
    let price_fr = decimal_to_scalar(price).map_err(|e| format!("price encoding: {e}"))?;
    let size_fr = decimal_to_scalar(size).map_err(|e| format!("size encoding: {e}"))?;

    let salt_bytes = hex::decode(&w.salt_hex).map_err(|e| format!("bad salt hex: {e}"))?;
    let salt = bytes_to_scalar(&salt_bytes);

    let circuit = CommitmentPreimageCircuit {
        trader_id,
        side: side_fr,
        limit_price: price_fr,
        size: size_fr,
        salt,
    };

    let mut rng = rand::rngs::OsRng;
    let result = setup_and_prove(&circuit, &mut rng).map_err(|e| format!("prove: {e}"))?;

    let mut commitment_bytes = Vec::new();
    result
        .commitment
        .serialize_with_mode(&mut commitment_bytes, Compress::Yes)
        .map_err(|e| format!("serialize commitment: {e}"))?;

    Ok(ProveResult {
        proof: result.proof_bytes,
        vk: result.vk_bytes,
        commitment: commitment_bytes,
    })
}

#[derive(Debug)]
pub struct ProveResult {
    pub proof: Vec<u8>,
    pub vk: Vec<u8>,
    pub commitment: Vec<u8>,
}

#[cfg(feature = "wasm")]
mod wasm_bindings {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn prove_order_wasm(witness_json: &str) -> Result<js_sys::Uint8Array, JsError> {
        let result = super::prove_order(witness_json).map_err(|e| JsError::new(&e))?;

        let proof_len = result.proof.len() as u32;
        let vk_len = result.vk.len() as u32;
        let commitment_len = result.commitment.len() as u32;

        // Wire format: [proof_len(4) | vk_len(4) | commitment_len(4) | proof | vk | commitment]
        let total = 12 + proof_len + vk_len + commitment_len;
        let mut buf = Vec::with_capacity(total as usize);
        buf.extend_from_slice(&proof_len.to_le_bytes());
        buf.extend_from_slice(&vk_len.to_le_bytes());
        buf.extend_from_slice(&commitment_len.to_le_bytes());
        buf.extend_from_slice(&result.proof);
        buf.extend_from_slice(&result.vk);
        buf.extend_from_slice(&result.commitment);

        Ok(js_sys::Uint8Array::from(&buf[..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_zk::commitment_circuit::verify_proof;
    use dp_zk::pedersen::{commit_native, OrderCommitmentInput};

    fn sample_witness_json() -> String {
        serde_json::json!({
            "commitment_key": "aa".repeat(32),
            "side": 0,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string()
    }

    #[test]
    fn prove_and_verify_round_trip() {
        let result = prove_order(&sample_witness_json()).unwrap();

        let commitment =
            <Fr as CanonicalSerialize>::serialized_size(&Fr::from(0u64), Compress::Yes);
        assert_eq!(result.commitment.len(), commitment);

        let c: Fr = ark_serialize::CanonicalDeserialize::deserialize_with_mode(
            result.commitment.as_slice(),
            Compress::Yes,
            ark_serialize::Validate::Yes,
        )
        .unwrap();

        assert!(verify_proof(&result.vk, &result.proof, c).unwrap());
    }

    #[test]
    fn commitment_matches_native() {
        let key_hex = "aa".repeat(32);
        let key_bytes = hex::decode(&key_hex).unwrap();
        let trader_id = derive_trader_id(&key_bytes).unwrap();
        let salt_hex = "bb".repeat(32);
        let salt_bytes = hex::decode(&salt_hex).unwrap();
        let salt = bytes_to_scalar(&salt_bytes);

        let input = OrderCommitmentInput::from_decimals(
            trader_id,
            0,
            "100".parse().unwrap(),
            "10".parse().unwrap(),
            salt,
        )
        .unwrap();
        let native_commitment = commit_native(&input);

        let result = prove_order(&sample_witness_json()).unwrap();
        let proof_commitment: Fr = ark_serialize::CanonicalDeserialize::deserialize_with_mode(
            result.commitment.as_slice(),
            Compress::Yes,
            ark_serialize::Validate::Yes,
        )
        .unwrap();

        assert_eq!(native_commitment, proof_commitment);
    }

    #[test]
    fn rejects_invalid_side() {
        let json = serde_json::json!({
            "commitment_key": "aa".repeat(32),
            "side": 2,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        let err = prove_order(&json).unwrap_err();
        assert!(err.contains("side must be 0 or 1"));
    }

    #[test]
    fn rejects_bad_hex() {
        let json = serde_json::json!({
            "commitment_key": "not_hex",
            "side": 0,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        assert!(prove_order(&json).is_err());
    }

    #[test]
    fn rejects_negative_price() {
        let json = serde_json::json!({
            "commitment_key": "aa".repeat(32),
            "side": 0,
            "price": "-100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        let err = prove_order(&json).unwrap_err();
        assert!(err.contains("encoding"));
    }
}
