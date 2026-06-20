//! Orchestrates an order submission: derive trader id, decode the payload salt,
//! compute Poseidon commitment, ECIES-encrypt the payload.
//!
//! The proof slot is a placeholder for local development only. Production
//! servers verify per-order Groth16 proofs and reject this placeholder unless
//! they were started with the unsafe unverified-proof escape hatch.

use crate::commitment::{
    bytes32_to_scalar, commit_native, derive_trader_id, scalar_to_be_bytes, OrderCommitmentInput,
};
use crate::encrypt::encrypt_order;
use crate::error::ClientError;
use crate::payload::OrderPayload;

/// Development-only proof bytes accepted only by servers started with
/// `DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true`.
pub const PLACEHOLDER_PROOF: &[u8] = b"dp-client-v0";

#[derive(Clone, Debug)]
pub struct OrderSubmission {
    pub commitment: [u8; 32],
    pub proof: Vec<u8>,
    pub encrypted_payload: Vec<u8>,
    pub trader_id: [u8; 32],
    pub salt: [u8; 32],
}

pub fn prepare_order(
    operator_pubkey: &[u8],
    payload: &OrderPayload,
) -> Result<OrderSubmission, ClientError> {
    prepare_order_with_entropy(operator_pubkey, payload, &[0u8; 32])
}

pub fn prepare_order_with_entropy(
    operator_pubkey: &[u8],
    payload: &OrderPayload,
    _entropy: &[u8; 32],
) -> Result<OrderSubmission, ClientError> {
    if payload.commitment_key.trim().is_empty() {
        return Err(ClientError::InvalidPayload(
            "commitment_key must be non-empty".to_string(),
        ));
    }
    validate_trader(&payload.trader)?;

    let trader_addr = parse_trader_address(&payload.trader)?;
    let trader_id_fr = derive_trader_id(&trader_addr)?;
    let trader_id = scalar_to_be_bytes(trader_id_fr);
    let salt = parse_payload_salt(&payload.salt)?;
    let salt_fr = bytes32_to_scalar(&salt)?;

    let inp = OrderCommitmentInput::from_decimals(
        trader_id_fr,
        payload.side.as_u8(),
        payload.price,
        payload.size,
        salt_fr,
    )?;
    let commitment = scalar_to_be_bytes(commit_native(&inp));

    let encrypted_payload = encrypt_order(operator_pubkey, payload)?;

    Ok(OrderSubmission {
        commitment,
        proof: PLACEHOLDER_PROOF.to_vec(),
        encrypted_payload,
        trader_id,
        salt,
    })
}

fn validate_trader(trader: &str) -> Result<(), ClientError> {
    if trader.len() != 42
        || !trader.starts_with("0x")
        || !trader[2..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(ClientError::InvalidPayload(
            "trader must be a 0x-prefixed 20-byte address".to_string(),
        ));
    }
    Ok(())
}

fn parse_trader_address(trader: &str) -> Result<[u8; 20], ClientError> {
    validate_trader(trader)?;
    let bytes = hex::decode(&trader[2..])?;
    bytes.try_into().map_err(|_| {
        ClientError::InvalidPayload("trader must be a 0x-prefixed 20-byte address".to_string())
    })
}

fn parse_payload_salt(salt_hex: &str) -> Result<[u8; 32], ClientError> {
    if salt_hex.len() != 64
        || !salt_hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ClientError::InvalidPayload(
            "salt must be a 32-byte lowercase hex string".to_string(),
        ));
    }
    let bytes = hex::decode(salt_hex)?;
    bytes.try_into().map_err(|_| {
        ClientError::InvalidPayload("salt must be a 32-byte lowercase hex string".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::Side;
    use rust_decimal::Decimal;

    fn fake_pubkey() -> Vec<u8> {
        // Deterministic SEC1 secp256k1 pubkey for non-decryption-path tests.
        use k256::ecdsa::SigningKey;
        let sk = SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
        sk.verifying_key().to_sec1_bytes().to_vec()
    }

    fn sample() -> OrderPayload {
        OrderPayload {
            trader: "0x0000000000000000000000000000000000000000".into(),
            pair: "ETH-USD".into(),
            side: Side::Buy,
            price: Decimal::new(250000, 2),
            size: Decimal::new(10, 1),
            commitment_key: "abc123".into(),
            salt: "01".repeat(32),
            ttl: 5_000_000_000,
        }
    }

    #[test]
    fn prepare_is_deterministic_given_entropy() {
        let pk = fake_pubkey();
        let p = sample();
        let entropy = [9u8; 32];
        let a = prepare_order_with_entropy(&pk, &p, &entropy).unwrap();
        let b = prepare_order_with_entropy(&pk, &p, &entropy).unwrap();
        // Commitment + trader_id + salt are deterministic; ciphertext is not
        // (ECIES adds an ephemeral pubkey + nonce per encryption).
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.trader_id, b.trader_id);
        assert_eq!(a.salt, b.salt);
        assert_ne!(a.encrypted_payload, b.encrypted_payload);
    }

    #[test]
    fn payload_salt_perturbs_commitment() {
        let pk = fake_pubkey();
        let a = prepare_order(&pk, &sample()).unwrap();
        let mut p = sample();
        p.salt = "02".repeat(32);
        let b = prepare_order(&pk, &p).unwrap();
        assert_ne!(a.commitment, b.commitment);
        assert_ne!(a.salt, b.salt);
    }

    #[test]
    fn trader_address_perturbs_commitment() {
        let pk = fake_pubkey();
        let a = prepare_order(&pk, &sample()).unwrap();
        let mut p = sample();
        p.trader = "0x1111111111111111111111111111111111111111".into();
        let b = prepare_order(&pk, &p).unwrap();
        assert_ne!(a.commitment, b.commitment);
        assert_ne!(a.trader_id, b.trader_id);
    }

    #[test]
    fn commitment_key_does_not_perturb_commitment() {
        let pk = fake_pubkey();
        let a = prepare_order(&pk, &sample()).unwrap();
        let mut p = sample();
        p.commitment_key = "different-payload-entropy".into();
        let b = prepare_order(&pk, &p).unwrap();
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.trader_id, b.trader_id);
        assert_ne!(a.encrypted_payload, b.encrypted_payload);
    }

    #[test]
    fn malformed_payload_salt_is_rejected() {
        let pk = fake_pubkey();
        for salt in ["".to_string(), "AB".repeat(32), "ab".repeat(31)] {
            let mut p = sample();
            p.salt = salt;
            let err = prepare_order(&pk, &p).unwrap_err();
            assert!(err.to_string().contains("salt"));
        }
    }

    #[test]
    fn non_canonical_payload_salt_is_rejected() {
        use ark_bn254::Fr;
        use ark_ff::{BigInteger, PrimeField};

        let pk = fake_pubkey();
        let mut p = sample();
        p.salt = hex::encode(Fr::MODULUS.to_bytes_be());

        let err = prepare_order(&pk, &p).unwrap_err();

        assert!(err.to_string().contains("not canonical"), "got {err}");
    }

    #[test]
    fn malformed_trader_is_rejected() {
        let pk = fake_pubkey();
        let mut p = sample();
        p.trader = "0xdead".into();
        let err = prepare_order(&pk, &p).unwrap_err();
        assert!(err.to_string().contains("trader"));
    }

    #[test]
    fn placeholder_proof_passes_api_validation() {
        // Non-empty + under 64 KiB; mirrors dp-api validation.rs.
        assert!(!PLACEHOLDER_PROOF.is_empty());
        assert!(PLACEHOLDER_PROOF.len() <= 64 * 1024);
    }
}
