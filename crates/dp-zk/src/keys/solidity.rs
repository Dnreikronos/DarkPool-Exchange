//! Convert an arkworks `VerifyingKey<Bn254>` into a JSON layout consumable
//! by `Groth16Verifier.sol`.
//!
//! G2 packing matches the precompile order used in
//! `contracts/src/Groth16Verifier.sol` (the `[c1, c0]` ordering for each
//! Fq2 coordinate, see lines 126–150).

use ark_bn254::{Bn254, Fq, G1Affine, G2Affine};
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

fn fq_to_hex(f: &Fq) -> String {
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
    // Solidity precompile expects Fq2 coordinates as [c1, c0].
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
