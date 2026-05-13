//! Convert an arkworks `VerifyingKey<Bn254>` into a JSON layout consumable
//! by `Groth16Verifier.sol`.
//!
//! G2 packing matches the precompile order used in
//! `contracts/src/Groth16Verifier.sol` (the `[c1, c0]` ordering for each
//! Fq2 coordinate, see lines 126–150).

use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::VerifyingKey;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct SolidityVk {
    pub alpha1: [String; 2],
    pub beta2: [[String; 2]; 2],
    pub gamma2: [[String; 2]; 2],
    pub delta2: [[String; 2]; 2],
    pub ic: Vec<[String; 2]>,
}

pub fn fq_to_hex<F: PrimeField>(f: &F) -> String {
    let bytes = f.into_bigint().to_bytes_be();
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in &bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn g1_xy(p: &G1Affine) -> [String; 2] {
    let (x, y) = p.xy().expect("G1 point at infinity in VK");
    [fq_to_hex(&x), fq_to_hex(&y)]
}

fn g2_xy(p: &G2Affine) -> [[String; 2]; 2] {
    let (x, y) = p.xy().expect("G2 point at infinity in VK");
    [
        [fq_to_hex(&x.c1), fq_to_hex(&x.c0)],
        [fq_to_hex(&y.c1), fq_to_hex(&y.c0)],
    ]
}

pub fn vk_to_solidity(vk: &VerifyingKey<Bn254>) -> SolidityVk {
    SolidityVk {
        alpha1: g1_xy(&vk.alpha_g1),
        beta2: g2_xy(&vk.beta_g2),
        gamma2: g2_xy(&vk.gamma_g2),
        delta2: g2_xy(&vk.delta_g2),
        ic: vk.gamma_abc_g1.iter().map(g1_xy).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bn254::{Fq, Fr};

    #[test]
    fn fq_to_hex_zero() {
        let z = Fq::from(0u64);
        let h = fq_to_hex(&z);
        assert!(h.starts_with("0x"));
        assert!(h.ends_with("00"));
        assert_eq!(h.len(), 66);
    }

    #[test]
    fn fq_to_hex_one() {
        let one = Fr::from(1u64);
        let h = fq_to_hex(&one);
        assert!(h.starts_with("0x"));
        assert!(h.ends_with("01"));
    }

    #[test]
    fn fq_to_hex_deterministic() {
        let v = Fq::from(42u64);
        assert_eq!(fq_to_hex(&v), fq_to_hex(&v));
    }

    #[test]
    fn g1_xy_valid_point() {
        let g = G1Affine::generator();
        let [x, y] = g1_xy(&g);
        assert!(x.starts_with("0x"));
        assert!(y.starts_with("0x"));
        assert_eq!(x.len(), 66);
        assert_eq!(y.len(), 66);
    }

    #[test]
    #[should_panic(expected = "G1 point at infinity")]
    fn g1_xy_panics_on_infinity() {
        g1_xy(&G1Affine::zero());
    }

    #[test]
    fn g2_xy_valid_point() {
        let g = G2Affine::generator();
        let coords = g2_xy(&g);
        for pair in &coords {
            for s in pair {
                assert!(s.starts_with("0x"));
                assert_eq!(s.len(), 66);
            }
        }
    }

    #[test]
    #[should_panic(expected = "G2 point at infinity")]
    fn g2_xy_panics_on_infinity() {
        g2_xy(&G2Affine::zero());
    }

    #[test]
    fn vk_to_solidity_round_trip() {
        use ark_std::rand::rngs::StdRng;
        use ark_std::rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(99);
        let (_, vk) = crate::circuit::setup(1, &mut rng).expect("setup");
        let sol = vk_to_solidity(&vk);

        assert_eq!(sol.alpha1.len(), 2);
        assert_eq!(sol.beta2.len(), 2);
        assert_eq!(sol.gamma2.len(), 2);
        assert_eq!(sol.delta2.len(), 2);
        assert!(!sol.ic.is_empty());

        for s in &sol.alpha1 {
            assert!(s.starts_with("0x"));
        }

        let json = serde_json::to_string(&sol).expect("serialize");
        assert!(json.contains("alpha1"));
        assert!(json.contains("ic"));
    }
}
