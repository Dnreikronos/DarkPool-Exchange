//! Canonical Poseidon parameters for DarkPool commitments.
//!
//! This crate is intentionally small so both `dp-zk` and the standalone
//! `dp-client` crate can share one BN254 Fr Poseidon configuration without
//! making the client depend on the Groth16/IVC stack.

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::poseidon::{find_poseidon_ark_and_mds, PoseidonConfig};
use ark_ff::PrimeField;

pub const POSEIDON_FULL_ROUNDS: usize = 8;
pub const POSEIDON_PARTIAL_ROUNDS: usize = 57;
pub const POSEIDON_ALPHA: u64 = 5;
pub const POSEIDON_RATE: usize = 2;
pub const POSEIDON_CAPACITY: usize = 1;

/// Poseidon parameters for BN254 Fr, rate 2 capacity 1, 8 full + 57 partial
/// rounds, and the standard x^5 S-box.
///
/// The generated ark/MDS values are regression-tested against the independent
/// HadesHash/Python reference constants from `poseidon-hash==0.1.4`.
pub fn poseidon_config() -> PoseidonConfig<Fr> {
    let modulus_bits = <Fr as PrimeField>::MODULUS_BIT_SIZE as u64;
    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
        modulus_bits,
        POSEIDON_RATE,
        POSEIDON_FULL_ROUNDS as u64,
        POSEIDON_PARTIAL_ROUNDS as u64,
        0,
    );

    PoseidonConfig::new(
        POSEIDON_FULL_ROUNDS,
        POSEIDON_PARTIAL_ROUNDS,
        POSEIDON_ALPHA,
        mds,
        ark,
        POSEIDON_RATE,
        POSEIDON_CAPACITY,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
    use ark_crypto_primitives::sponge::CryptographicSponge;
    use ark_ff::BigInteger;
    use sha2::{Digest, Sha256};

    const PARAMETER_DIGEST_HEX: &str =
        "d580f0ebf5aec8825a0212c9db8fd20135155e1352e68809e2d7f2bc5440b86f";

    fn fr_to_bytes32(f: Fr) -> [u8; 32] {
        let bytes = f.into_bigint().to_bytes_be();
        let mut out = [0u8; 32];
        let take = bytes.len().min(32);
        out[32 - take..].copy_from_slice(&bytes[bytes.len() - take..]);
        out
    }

    fn hex_to_bytes32(hex: &str) -> [u8; 32] {
        let s = hex.strip_prefix("0x").unwrap_or(hex);
        assert_eq!(s.len(), 64);
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    fn fr_from_hex(hex: &str) -> Fr {
        Fr::from_be_bytes_mod_order(&hex_to_bytes32(hex))
    }

    fn sponge_hash(inputs: &[Fr]) -> [u8; 32] {
        let cfg = poseidon_config();
        let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
        sponge.absorb(&inputs.to_vec());
        fr_to_bytes32(sponge.squeeze_field_elements::<Fr>(1)[0])
    }

    #[test]
    fn parameters_match_hadeshash_reference_digest() {
        let cfg = poseidon_config();
        assert_eq!(cfg.full_rounds, POSEIDON_FULL_ROUNDS);
        assert_eq!(cfg.partial_rounds, POSEIDON_PARTIAL_ROUNDS);
        assert_eq!(cfg.alpha, POSEIDON_ALPHA);
        assert_eq!(cfg.rate, POSEIDON_RATE);
        assert_eq!(cfg.capacity, POSEIDON_CAPACITY);

        let mut hasher = Sha256::new();
        for row in &cfg.ark {
            for value in row {
                hasher.update(fr_to_bytes32(*value));
            }
        }
        for row in &cfg.mds {
            for value in row {
                hasher.update(fr_to_bytes32(*value));
            }
        }

        assert_eq!(format!("{:x}", hasher.finalize()), PARAMETER_DIGEST_HEX);
    }

    #[test]
    fn selected_parameters_match_hadeshash_reference_values() {
        let cfg = poseidon_config();
        assert_eq!(
            cfg.ark[0][0],
            fr_from_hex("0ee9a592ba9a9518d05986d656f40c2114c4993c11bb29938d21d47304cd8e6e")
        );
        assert_eq!(
            cfg.ark[64][2],
            fr_from_hex("1da55cc900f0d21f4a3e694391918a1b3c23b2ac773c6b3ef88e2e4228325161")
        );
        assert_eq!(
            cfg.mds[0][0],
            fr_from_hex("109b7f411ba0e4c9b2b70caf5c36a7b194be7c11ad24378bfedb68592ba8118b")
        );
        assert_eq!(
            cfg.mds[2][2],
            fr_from_hex("19a3fc0a56702bf417ba7fee3802593fa644470307043f7773279cd71d25d5e0")
        );
    }

    #[test]
    fn sponge_vectors_match_hadeshash_reference() {
        let cases = [
            (
                "empty",
                vec![],
                "13a545a13f1d91dddb87f46679dfaec0900ce24791a924bee7fa4d69a9569d85",
            ),
            (
                "one",
                vec![Fr::from(1u64)],
                "1aca579a4fc78f50613d9709982feef7ec9e080273beb2fcde7d7a5d9226d2a0",
            ),
            (
                "two",
                vec![Fr::from(1u64), Fr::from(2u64)],
                "0fca49b798923ab0239de1c9e7a4a9a2210312b6a2f616d18b5a87f9b628ae29",
            ),
            (
                "three",
                vec![Fr::from(1u64), Fr::from(2u64), Fr::from(3u64)],
                "1e706b0afc828a5262be1773734e80df7fa9c0aa25c8fd5dfb008122a62e65ca",
            ),
            (
                "alice-trader-id",
                vec![Fr::from_be_bytes_mod_order(b"alice")],
                "0b5ddce136b4437b7df9a9c9a15738d2bf060737790af21312f0e3d6f0125a2e",
            ),
            (
                "order-commitment-shape",
                vec![
                    Fr::from(7u64),
                    Fr::from(0u64),
                    Fr::from(10_000_000_000u64),
                    Fr::from(1_000_000_000u64),
                    Fr::from(42u64),
                ],
                "1d72f6f4466286a9aa862000ee503f2aba292924007b155589f7df1cfe058d38",
            ),
        ];

        for (label, input, expected) in cases {
            assert_eq!(sponge_hash(&input), hex_to_bytes32(expected), "{label}");
        }
    }
}
