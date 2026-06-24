//! Poseidon order-leg commitment. Uses the shared `dp-poseidon` parameter
//! source so client- and engine-side commitments agree byte-for-byte.
//!
//! Kept in this crate (rather than depending on `dp-zk`) to avoid pulling
//! ark-groth16 / rayon into the WASM build.

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
use ark_crypto_primitives::sponge::CryptographicSponge;
use ark_ff::{BigInteger, One, PrimeField, Zero};
use dp_poseidon::poseidon_config;
use rust_decimal::Decimal;

use crate::encoding::decimal_to_scalar;
use crate::error::ClientError;

#[derive(Clone, Debug)]
pub struct OrderCommitmentInput {
    pub trader_id: Fr,
    pub side: Fr,
    pub limit_price: Fr,
    pub size: Fr,
    pub salt: Fr,
}

impl OrderCommitmentInput {
    pub fn from_decimals(
        trader_id: Fr,
        side: u8,
        limit_price: Decimal,
        size: Decimal,
        salt: Fr,
    ) -> Result<Self, ClientError> {
        Ok(Self {
            trader_id,
            side: match side {
                0 => Fr::zero(),
                1 => Fr::one(),
                other => {
                    return Err(ClientError::InvalidPayload(format!(
                        "side must be 0 or 1, got {other}"
                    )))
                }
            },
            limit_price: decimal_to_scalar(limit_price)?,
            size: decimal_to_scalar(size)?,
            salt,
        })
    }

    pub fn as_field_elements(&self) -> [Fr; 5] {
        [
            self.trader_id,
            self.side,
            self.limit_price,
            self.size,
            self.salt,
        ]
    }
}

pub fn commit_native(input: &OrderCommitmentInput) -> Fr {
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&input.as_field_elements().to_vec());
    sponge.squeeze_field_elements::<Fr>(1)[0]
}

pub fn derive_trader_id(commitment_key_bytes: &[u8]) -> Result<Fr, ClientError> {
    let scalar = bytes_to_scalar(commitment_key_bytes)?;
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&vec![scalar]);
    Ok(sponge.squeeze_field_elements::<Fr>(1)[0])
}

pub fn bytes_to_scalar(b: &[u8]) -> Result<Fr, ClientError> {
    if b.len() > 32 {
        return Err(ClientError::Encoding(format!(
            "field element encoding must be at most 32 bytes, got {}",
            b.len()
        )));
    }

    let scalar = Fr::from_be_bytes_mod_order(b);
    if b.len() == 32 && scalar_to_be_bytes(scalar) != b {
        return Err(ClientError::Encoding(
            "field element encoding is not canonical for BN254 Fr".to_string(),
        ));
    }

    Ok(scalar)
}

pub fn bytes32_to_scalar(b: &[u8]) -> Result<Fr, ClientError> {
    if b.len() != 32 {
        return Err(ClientError::Encoding(format!(
            "field element encoding must be exactly 32 bytes, got {}",
            b.len()
        )));
    }
    bytes_to_scalar(b)
}

pub fn scalar_to_be_bytes(f: Fr) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let take = bytes.len().min(32);
    out[32 - take..].copy_from_slice(&bytes[bytes.len() - take..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_deterministic() {
        let inp = OrderCommitmentInput::from_decimals(
            Fr::from(7u64),
            0,
            Decimal::from(100),
            Decimal::from(10),
            Fr::from(42u64),
        )
        .unwrap();
        assert_eq!(commit_native(&inp), commit_native(&inp));
    }

    #[test]
    fn commit_changes_with_salt() {
        let a = OrderCommitmentInput::from_decimals(
            Fr::from(7u64),
            0,
            Decimal::from(100),
            Decimal::from(10),
            Fr::from(1u64),
        )
        .unwrap();
        let b = OrderCommitmentInput::from_decimals(
            Fr::from(7u64),
            0,
            Decimal::from(100),
            Decimal::from(10),
            Fr::from(2u64),
        )
        .unwrap();
        assert_ne!(commit_native(&a), commit_native(&b));
    }

    #[test]
    fn trader_id_round_trips_through_bytes() {
        let f = derive_trader_id(b"alice").unwrap();
        let bytes = scalar_to_be_bytes(f);
        let recovered = bytes32_to_scalar(&bytes).unwrap();
        assert_eq!(recovered, f);
    }

    #[test]
    fn bytes32_rejects_non_canonical_modulus() {
        let modulus = Fr::MODULUS.to_bytes_be();
        assert_eq!(modulus.len(), 32);
        assert!(bytes32_to_scalar(&modulus).is_err());
    }
}
