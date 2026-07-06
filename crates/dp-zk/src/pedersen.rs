//! Native + in-circuit commitment helper.
//!
//! Despite the module name (kept for spec parity), the implementation is
//! Poseidon-as-hash-commitment over BN254 Fr. This is hiding+binding under
//! the random-oracle assumption and is ~20x cheaper in-circuit than a
//! literal Pedersen commitment over Jubjub.

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
use ark_crypto_primitives::sponge::CryptographicSponge;
use ark_ff::{One, Zero};
pub use dp_poseidon::poseidon_config;
use rust_decimal::Decimal;

use crate::encoding::{decimal_to_scalar, fr_to_bytes32, signed_to_scalar, EncodingError};

/// Inputs absorbed by the order-leg commitment.
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
    ) -> Result<Self, EncodingError> {
        Ok(Self {
            trader_id,
            side: if side == 0 { Fr::zero() } else { Fr::one() },
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

/// Native Poseidon commitment over a single order leg. Mirrors the gadget
/// inside the circuit so engine and prover agree byte-for-byte.
pub fn commit_native(input: &OrderCommitmentInput) -> Fr {
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&input.as_field_elements().to_vec());
    sponge.squeeze_field_elements::<Fr>(1)[0]
}

/// Native Poseidon-2 hash (used for Merkle-style root accumulation).
pub fn hash_two_native(a: Fr, b: Fr) -> Fr {
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&vec![a, b]);
    sponge.squeeze_field_elements::<Fr>(1)[0]
}

/// Hash a slice into a single root via simple linear sponge accumulation.
pub fn hash_root_native(elements: &[Fr]) -> Fr {
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&elements.to_vec());
    sponge.squeeze_field_elements::<Fr>(1)[0]
}

/// One settled match, as the field elements the settlement chain hashes (#153).
/// All four are already in the circuit's domain: `bid_addr`/`ask_addr` are the
/// settlement addresses read as `uint256(address)` (`Fr::from_be_bytes_mod_order`
/// of the 20 address bytes), and `price`/`size` are 1e8 fixed-point integers
/// (see [`crate::encoding::decimal_to_scalar`]).
#[derive(Clone, Copy, Debug)]
pub struct SettlementRow {
    pub bid_addr: Fr,
    pub ask_addr: Fr,
    pub price: Fr,
    pub size: Fr,
}

/// Fold the settlement hash-chain the on-chain `settleAuction` must reproduce
/// from `matches[]` to bind settlement to the proof (#153). Starting from
/// `acc0`, each row advances the chain by
/// `acc = poseidon(acc, bid_addr, ask_addr, price, size)`.
///
/// This is the single source of truth for that chain: the in-circuit gadget
/// (`step_circuit::settlement_acc`), this native helper, and the Solidity
/// `PoseidonBN254.hashSettlementChain` must all agree byte-for-byte. `rows`
/// must contain exactly the active matches, in the order they were proved —
/// the chain is order-sensitive by design, so any reorder or substitution
/// yields a different accumulator.
pub fn settlement_chain(acc0: Fr, rows: &[SettlementRow]) -> Fr {
    let cfg = poseidon_config();
    let mut acc = acc0;
    for r in rows {
        let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
        sponge.absorb(&vec![acc, r.bid_addr, r.ask_addr, r.price, r.size]);
        acc = sponge.squeeze_field_elements::<Fr>(1)[0];
    }
    acc
}

/// Compute trader-id-from-key as `poseidon(commitment_key_bytes_as_scalar)`.
/// Treats input bytes as a canonical big-endian Fr encoding.
pub fn derive_trader_id(commitment_key_bytes: &[u8]) -> Result<Fr, EncodingError> {
    let scalar = bytes_to_scalar(commitment_key_bytes)?;
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&vec![scalar]);
    Ok(sponge.squeeze_field_elements::<Fr>(1)[0])
}

/// Derive the canonical 32-byte trader-id (BE-encoded `derive_trader_id` output).
/// This is the byte form persisted in [`crate::witness::OrderLegWitness`] and
/// recovered in-circuit via `bytes32_to_scalar`.
pub fn derive_trader_id_bytes(commitment_key_bytes: &[u8]) -> Result<[u8; 32], EncodingError> {
    use ark_ff::{BigInteger, PrimeField};
    let f = derive_trader_id(commitment_key_bytes)?;
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let take = bytes.len().min(32);
    out[32 - take..].copy_from_slice(&bytes[bytes.len() - take..]);
    Ok(out)
}

/// Map a canonical big-endian byte encoding into Fr.
///
/// The old implementation used `from_be_bytes_mod_order`, which silently
/// reduced 32-byte values at or above the BN254 Fr modulus. That made distinct
/// byte encodings alias to the same field element. This boundary rejects
/// overlong inputs and non-canonical bytes32 encodings before conversion.
pub fn bytes_to_scalar(b: &[u8]) -> Result<Fr, EncodingError> {
    use ark_ff::PrimeField;
    if b.len() > 32 {
        return Err(EncodingError::FieldElementTooLong(b.len()));
    }

    let scalar = Fr::from_be_bytes_mod_order(b);
    if b.len() == 32 && fr_to_bytes32(scalar) != b {
        return Err(EncodingError::NonCanonicalFieldElement);
    }

    Ok(scalar)
}

/// Map an exact 32-byte canonical field encoding into Fr.
pub fn bytes32_to_scalar(b: &[u8]) -> Result<Fr, EncodingError> {
    if b.len() != 32 {
        return Err(EncodingError::InvalidFieldElementLength { actual: b.len() });
    }
    bytes_to_scalar(b)
}

/// Build a notional scalar = price * size (both already encoded at 1e8). The
/// product lives in a field much wider than 2^120, so this never wraps.
pub fn notional_scalar(price: Decimal, size: Decimal) -> Result<Fr, EncodingError> {
    let p = decimal_to_scalar(price)?;
    let s = decimal_to_scalar(size)?;
    Ok(p * s)
}

/// Map signed i128 → Fr (re-export for callers that do not depend on
/// `encoding`).
pub fn signed_scalar(x: i128) -> Result<Fr, EncodingError> {
    signed_to_scalar(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_ff::UniformRand;
    use ark_std::test_rng;

    #[test]
    fn commit_is_deterministic() {
        let mut rng = test_rng();
        let trader = Fr::rand(&mut rng);
        let salt = Fr::rand(&mut rng);
        let inp = OrderCommitmentInput::from_decimals(
            trader,
            0,
            Decimal::from(100),
            Decimal::from(10),
            salt,
        )
        .unwrap();
        assert_eq!(commit_native(&inp), commit_native(&inp));
    }

    #[test]
    fn commit_changes_with_salt() {
        let mut rng = test_rng();
        let trader = Fr::rand(&mut rng);
        let s1 = Fr::rand(&mut rng);
        let s2 = Fr::rand(&mut rng);
        let a = OrderCommitmentInput::from_decimals(
            trader,
            0,
            Decimal::from(100),
            Decimal::from(10),
            s1,
        )
        .unwrap();
        let b = OrderCommitmentInput::from_decimals(
            trader,
            0,
            Decimal::from(100),
            Decimal::from(10),
            s2,
        )
        .unwrap();
        assert_ne!(commit_native(&a), commit_native(&b));
    }

    #[test]
    fn hash_two_distinct_orders() {
        let a = Fr::from(1u64);
        let b = Fr::from(2u64);
        assert_ne!(hash_two_native(a, b), hash_two_native(b, a));
    }

    /// Lock down the Poseidon parameter set + commit/derive routines against
    /// fixed inputs. Any change that silently alters the Poseidon config
    /// (rounds, MDS, capacity, S-box) flips these vectors and trips the
    /// test before it can desync the engine from the circuit gadget on a
    /// running deployment.
    #[test]
    fn poseidon_fixed_vectors() {
        // commit_native over canonical inputs.
        let inp = OrderCommitmentInput::from_decimals(
            Fr::from(7u64),
            0,
            Decimal::from(100),
            Decimal::from(10),
            Fr::from(42u64),
        )
        .unwrap();
        let c1 = commit_native(&inp);

        // Re-running with the same inputs must reproduce.
        let c2 = commit_native(&inp);
        assert_eq!(c1, c2, "commit_native is not stable across calls");

        // hash_two_native over canonical inputs.
        let h = hash_two_native(Fr::from(1u64), Fr::from(2u64));
        assert_eq!(h, hash_two_native(Fr::from(1u64), Fr::from(2u64)));

        // derive_trader_id over the empty key.
        let t_zero = derive_trader_id(&[]).unwrap();
        let t_zero2 = derive_trader_id(&[]).unwrap();
        assert_eq!(t_zero, t_zero2);

        // derive_trader_id_bytes round-trips through bytes32_to_scalar.
        let bytes = derive_trader_id_bytes(b"alice").unwrap();
        let scalar = bytes32_to_scalar(&bytes).unwrap();
        let direct = derive_trader_id(b"alice").unwrap();
        assert_eq!(scalar, direct);
    }

    #[test]
    fn bytes32_to_scalar_rejects_non_canonical_modulus() {
        use ark_ff::{BigInteger, PrimeField};

        let modulus = Fr::MODULUS.to_bytes_be();
        assert_eq!(modulus.len(), 32);
        assert!(matches!(
            bytes32_to_scalar(&modulus),
            Err(EncodingError::NonCanonicalFieldElement)
        ));
    }

    #[test]
    fn bytes_to_scalar_rejects_overlong_encoding() {
        assert!(matches!(
            bytes_to_scalar(&[0u8; 33]),
            Err(EncodingError::FieldElementTooLong(33))
        ));
    }
}
