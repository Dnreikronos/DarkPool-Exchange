use ark_bn254::Fr;
use ark_serialize::{CanonicalSerialize, Compress};
use serde::Deserialize;

use dp_zk::commitment_circuit::{deserialize_pk, prove_with_key, CommitmentPreimageCircuit};
use dp_zk::encoding::decimal_to_scalar;
use dp_zk::pedersen::{bytes_to_scalar, derive_trader_id};

#[derive(Deserialize)]
struct WitnessInput {
    trader_addr: String,
    side: u8,
    price: String,
    size: String,
    salt_hex: String,
}

/// Prove knowledge of an order's commitment preimage against the canonical
/// proving key supplied by the caller.
///
/// `pk_bytes` is the versioned proving-key blob from the trusted-setup
/// ceremony (`dp-zk-cli setup-commitment-circuit`). The prover never runs
/// setup and never returns a verifying key: the verifier pins the canonical VK
/// from the same ceremony (issue #212). Output is `(proof, commitment)` only.
pub fn prove_order(witness_json: &str, pk_bytes: &[u8]) -> Result<ProveResult, String> {
    let w: WitnessInput =
        serde_json::from_str(witness_json).map_err(|e| format!("invalid witness JSON: {e}"))?;

    let trader_addr_bytes = decode_hex_field("trader_addr", &w.trader_addr)?;
    if trader_addr_bytes.len() != 20 {
        return Err(format!(
            "trader_addr must be 20 bytes, got {}",
            trader_addr_bytes.len()
        ));
    }
    // `trader_id == poseidon(trader_addr)` (#216): the circuit binds the proof
    // to the verified on-chain address, matching the engine's derivation.
    let trader_addr = bytes_to_scalar(&trader_addr_bytes);
    let trader_id =
        derive_trader_id(&trader_addr_bytes).map_err(|e| format!("derive_trader_id: {e}"))?;

    if w.side > 1 {
        return Err(format!("side must be 0 or 1, got {}", w.side));
    }
    let side_fr = Fr::from(w.side as u64);

    let price: rust_decimal::Decimal = w.price.parse().map_err(|e| format!("bad price: {e}"))?;
    let size: rust_decimal::Decimal = w.size.parse().map_err(|e| format!("bad size: {e}"))?;
    let price_fr = decimal_to_scalar(price).map_err(|e| format!("price encoding: {e}"))?;
    let size_fr = decimal_to_scalar(size).map_err(|e| format!("size encoding: {e}"))?;

    let salt_bytes = decode_hex_field("salt_hex", &w.salt_hex)?;
    if salt_bytes.len() != 32 {
        return Err(format!(
            "salt_hex must be 32 bytes, got {}",
            salt_bytes.len()
        ));
    }
    let salt = bytes_to_scalar(&salt_bytes);

    let circuit = CommitmentPreimageCircuit {
        trader_id,
        trader_addr,
        side: side_fr,
        limit_price: price_fr,
        size: size_fr,
        salt,
    };

    // Input is validated above before the (large) key is deserialized, so a
    // malformed witness fails fast without paying for key decode.
    let pk = deserialize_pk(pk_bytes).map_err(|e| format!("deserialize proving key: {e}"))?;

    let mut rng = rand::rngs::OsRng;
    let (publics, proof_bytes) =
        prove_with_key(&pk, &circuit, &mut rng).map_err(|e| format!("prove: {e}"))?;

    // The wire format carries only the commitment. The verifier re-derives the
    // nullifier public input from (commitment, salt) — both of which the engine
    // holds after decryption (#217) — so it need not cross the boundary here.
    // Transmitting the nullifier on the wire is deferred to the wire-format PR.
    let mut commitment_bytes = Vec::new();
    publics
        .commitment
        .serialize_with_mode(&mut commitment_bytes, Compress::Yes)
        .map_err(|e| format!("serialize commitment: {e}"))?;

    Ok(ProveResult {
        proof: proof_bytes,
        commitment: commitment_bytes,
    })
}

fn decode_hex_field(name: &str, value: &str) -> Result<Vec<u8>, String> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    hex::decode(trimmed).map_err(|e| format!("bad {name} hex: {e}"))
}

#[derive(Debug)]
pub struct ProveResult {
    pub proof: Vec<u8>,
    pub commitment: Vec<u8>,
}

#[cfg(feature = "wasm")]
mod wasm_bindings {
    use wasm_bindgen::prelude::*;

    /// `pk_bytes` is the canonical proving-key blob, loaded by the caller from
    /// the ceremony asset. Returns `[proof_len(4) | commitment_len(4) | proof |
    /// commitment]` — no verifying key crosses the boundary (issue #212).
    #[wasm_bindgen]
    pub fn prove_order_wasm(
        witness_json: &str,
        pk_bytes: &[u8],
    ) -> Result<js_sys::Uint8Array, JsError> {
        let result = super::prove_order(witness_json, pk_bytes).map_err(|e| JsError::new(&e))?;

        let proof_len = result.proof.len() as u32;
        let commitment_len = result.commitment.len() as u32;

        // Wire format: [proof_len(4) | commitment_len(4) | proof | commitment]
        let total = 8 + proof_len + commitment_len;
        let mut buf = Vec::with_capacity(total as usize);
        buf.extend_from_slice(&proof_len.to_le_bytes());
        buf.extend_from_slice(&commitment_len.to_le_bytes());
        buf.extend_from_slice(&result.proof);
        buf.extend_from_slice(&result.commitment);

        Ok(js_sys::Uint8Array::from(&buf[..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::SeedableRng;
    use dp_zk::commitment_circuit::{
        generate_keys, serialize_pk, serialize_vk, verify_proof, OrderProofPublics,
    };
    use dp_zk::pedersen::{commit_native, OrderCommitmentInput};

    /// Canonical (pk, vk) blobs from a one-time setup, mirroring what the
    /// ceremony CLI emits: the prover takes the pk, the verifier pins the vk.
    fn canonical_keys() -> (Vec<u8>, Vec<u8>) {
        let mut rng = ark_std::rand::rngs::StdRng::from_seed([7u8; 32]);
        let (pk, vk) = generate_keys(&mut rng).unwrap();
        (serialize_pk(&pk).unwrap(), serialize_vk(&vk).unwrap())
    }

    fn sample_witness_json() -> String {
        serde_json::json!({
            "trader_addr": "0x1111111111111111111111111111111111111111",
            "side": 0,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string()
    }

    #[test]
    fn prove_and_verify_round_trip() {
        let (pk_bytes, vk_bytes) = canonical_keys();
        let result = prove_order(&sample_witness_json(), &pk_bytes).unwrap();

        let commitment_size =
            <Fr as CanonicalSerialize>::serialized_size(&Fr::from(0u64), Compress::Yes);
        assert_eq!(result.commitment.len(), commitment_size);

        let c: Fr = ark_serialize::CanonicalDeserialize::deserialize_with_mode(
            result.commitment.as_slice(),
            Compress::Yes,
            ark_serialize::Validate::Yes,
        )
        .unwrap();

        // The verifier derives the nullifier public input from the commitment and
        // the (known) salt — the same value the circuit bound.
        let salt = bytes_to_scalar(&hex::decode("bb".repeat(32)).unwrap());
        let publics = OrderProofPublics::derive(c, salt);

        // Verify against the canonical VK — never a prover-supplied one.
        assert!(verify_proof(&vk_bytes, &result.proof, &publics).unwrap());
    }

    #[test]
    fn commitment_matches_native() {
        let trader_addr = hex::decode("1111111111111111111111111111111111111111").unwrap();
        let trader_id = derive_trader_id(&trader_addr).unwrap();
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

        let (pk_bytes, _vk_bytes) = canonical_keys();
        let result = prove_order(&sample_witness_json(), &pk_bytes).unwrap();
        let proof_commitment: Fr = ark_serialize::CanonicalDeserialize::deserialize_with_mode(
            result.commitment.as_slice(),
            Compress::Yes,
            ark_serialize::Validate::Yes,
        )
        .unwrap();

        assert_eq!(native_commitment, proof_commitment);
    }

    // Input validation runs before the proving key is touched, so these
    // rejections need no real key.
    #[test]
    fn rejects_invalid_side() {
        let json = serde_json::json!({
            "trader_addr": "0x1111111111111111111111111111111111111111",
            "side": 2,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        let err = prove_order(&json, &[]).unwrap_err();
        assert!(err.contains("side must be 0 or 1"));
    }

    #[test]
    fn rejects_bad_hex() {
        let json = serde_json::json!({
            "trader_addr": "not_hex",
            "side": 0,
            "price": "100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        assert!(prove_order(&json, &[]).is_err());
    }

    #[test]
    fn rejects_negative_price() {
        let json = serde_json::json!({
            "trader_addr": "0x1111111111111111111111111111111111111111",
            "side": 0,
            "price": "-100",
            "size": "10",
            "salt_hex": "bb".repeat(32)
        })
        .to_string();
        let err = prove_order(&json, &[]).unwrap_err();
        assert!(err.contains("encoding"));
    }

    /// #212 boundary at the WASM layer: a proof minted under a prover-chosen
    /// proving key verifies under that key's own VK, but the pinned canonical
    /// VK rejects it. Switching the prover off self-setup is what makes this
    /// hold — the prover can no longer hand the verifier a matching VK.
    #[test]
    fn proof_under_foreign_pk_rejected_by_canonical_vk() {
        let (_canonical_pk, canonical_vk) = canonical_keys();

        // A different keypair the prover could swap in.
        let mut rng = ark_std::rand::rngs::StdRng::from_seed([42u8; 32]);
        let (foreign_pk, _foreign_vk) = generate_keys(&mut rng).unwrap();
        let foreign_pk_bytes = serialize_pk(&foreign_pk).unwrap();

        let result = prove_order(&sample_witness_json(), &foreign_pk_bytes).unwrap();
        let c: Fr = ark_serialize::CanonicalDeserialize::deserialize_with_mode(
            result.commitment.as_slice(),
            Compress::Yes,
            ark_serialize::Validate::Yes,
        )
        .unwrap();

        let salt = bytes_to_scalar(&hex::decode("bb".repeat(32)).unwrap());
        let publics = OrderProofPublics::derive(c, salt);

        assert!(!verify_proof(&canonical_vk, &result.proof, &publics).unwrap());
    }
}
