//! Orchestrates an order submission: derive trader id, sample salt, compute
//! Poseidon commitment, ECIES-encrypt the payload.
//!
//! The engine recomputes its own commitment from the decrypted payload using a
//! per-boot salt the client cannot know (engine.rs:469-476), so the
//! commitment we emit here is for local record-keeping and future per-order
//! verification — not for content-binding the engine.
//!
//! The proof slot is a placeholder until per-order circuits exist; the
//! validation layer (dp-api) only checks non-emptiness and a 64 KiB upper
//! bound today.

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::commitment::{
    bytes_to_scalar, commit_native, derive_trader_id, scalar_to_be_bytes, OrderCommitmentInput,
};
use crate::encrypt::encrypt_order;
use crate::error::ClientError;
use crate::payload::OrderPayload;

pub const PLACEHOLDER_PROOF: &[u8] = b"dp-client-v0";
const SALT_DOMAIN_TAG: &[u8] = b"dp-client-salt";

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
    let mut rng = rand::thread_rng();
    let mut entropy = [0u8; 32];
    rng.fill_bytes(&mut entropy);
    prepare_order_with_entropy(operator_pubkey, payload, &entropy)
}

pub fn prepare_order_with_entropy(
    operator_pubkey: &[u8],
    payload: &OrderPayload,
    entropy: &[u8; 32],
) -> Result<OrderSubmission, ClientError> {
    let key_bytes = payload.commitment_key.as_bytes();

    let trader_id_fr = derive_trader_id(key_bytes);
    let trader_id = scalar_to_be_bytes(trader_id_fr);

    let mut hasher = Sha256::new();
    hasher.update(SALT_DOMAIN_TAG);
    hasher.update(key_bytes);
    hasher.update(entropy);
    let salt: [u8; 32] = hasher.finalize().into();
    let salt_fr = bytes_to_scalar(&salt);

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
            pair: "ETH-USD".into(),
            side: Side::Buy,
            price: Decimal::new(250000, 2),
            size: Decimal::new(10, 1),
            commitment_key: "abc123".into(),
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
    fn entropy_perturbs_commitment() {
        let pk = fake_pubkey();
        let p = sample();
        let a = prepare_order_with_entropy(&pk, &p, &[1u8; 32]).unwrap();
        let b = prepare_order_with_entropy(&pk, &p, &[2u8; 32]).unwrap();
        assert_ne!(a.commitment, b.commitment);
        assert_ne!(a.salt, b.salt);
    }

    #[test]
    fn placeholder_proof_passes_api_validation() {
        // Non-empty + under 64 KiB; mirrors dp-api validation.rs.
        assert!(!PLACEHOLDER_PROOF.is_empty());
        assert!(PLACEHOLDER_PROOF.len() <= 64 * 1024);
    }
}
