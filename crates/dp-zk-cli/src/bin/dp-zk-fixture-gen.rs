//! Generate deterministic Groth16 fixtures (vk.json + proof.json) for the
//! Solidity verifier tests in `contracts/test/fixtures/`.
//!
//! Output layout (see `contracts/test/Groth16Verifier.t.sol`):
//!
//! - `vk.json`: alpha1, beta2, gamma2, delta2, ic — hex strings, G2 packed
//!   `[c1, c0]` per Fq2 coord (precompile order).
//! - `proof.json`: `a`, `b`, `c` hex G1/G2 coords + `publicInputs` (hex Fr).

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use ark_bn254::Bn254;
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::{Groth16, Proof};
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use ark_snark::SNARK;
use ark_std::rand::rngs::StdRng;
use ark_std::rand::SeedableRng;
use clap::Parser;
use dp_zk::circuit::{prove, setup};
use dp_zk::pedersen::derive_trader_id;
use dp_zk::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use dp_zk::BatchProofCircuit;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "dp-zk-fixture-gen", about = "Emit Groth16 fixtures for Solidity tests")]
struct Args {
    /// Output directory. vk.json + proof.json will be written here.
    #[arg(long)]
    out_dir: PathBuf,
    /// Deterministic seed (defaults to 42 — keep stable so fixtures hash-pin).
    #[arg(long, default_value_t = 42u64)]
    seed: u64,
    /// Circuit batch size.
    #[arg(long, default_value_t = 2usize)]
    batch_size: usize,
}

#[derive(Serialize)]
struct ProofJson {
    a: [String; 2],
    b: [[String; 2]; 2],
    c: [String; 2],
    public_inputs: Vec<String>,
}

fn field_hex<F: PrimeField>(f: &F) -> String {
    let bytes = f.into_bigint().to_bytes_be();
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in &bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn trader_id_hex(commitment_key: &str) -> String {
    let f = derive_trader_id(commitment_key.as_bytes()).expect("derive_trader_id");
    let mut bytes = f.into_bigint().to_bytes_be();
    while bytes.len() < 32 {
        bytes.insert(0, 0);
    }
    hex::encode(bytes)
}

fn sample_witness() -> (BatchWitness, Vec<Decimal>, Vec<Decimal>) {
    let bid_key = "bid_key".to_string();
    let ask_key = "ask_key".to_string();
    let m = MatchWitness {
        bid: OrderLegWitness {
            trader_id: trader_id_hex(&bid_key),
            salt: "22".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(105),
            order_size: Decimal::from(10),
            side: 0,
            commitment_key: bid_key,
        },
        ask: OrderLegWitness {
            trader_id: trader_id_hex(&ask_key),
            salt: "44".repeat(32),
            balance: Decimal::from(1_000_000),
            position: "0".into(),
            limit_price: Decimal::from(95),
            order_size: Decimal::from(10),
            side: 1,
            commitment_key: ask_key,
        },
    };
    let w = BatchWitness {
        batch_id: Uuid::nil(),
        auction_id: Uuid::nil(),
        matches: vec![m],
        policy: DEFAULT_POLICY.into_policy(),
    };
    (w, vec![Decimal::from(100)], vec![Decimal::from(10)])
}


fn main() -> ExitCode {
    let args = Args::parse();

    if let Err(e) = fs::create_dir_all(&args.out_dir) {
        eprintln!("create_dir_all: {e}");
        return ExitCode::from(2);
    }

    let mut rng = StdRng::seed_from_u64(args.seed);
    let (pk, vk) = match setup(args.batch_size, &mut rng) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("setup: {e}");
            return ExitCode::from(2);
        }
    };

    let sol_vk = dp_zk::keys::vk_to_solidity(&vk);
    let vk_json = match serde_json::to_string_pretty(&sol_vk) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("vk serialize: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = fs::write(args.out_dir.join("vk.json"), vk_json.as_bytes()) {
        eprintln!("write vk.json: {e}");
        return ExitCode::from(2);
    }

    let (witness, prices, sizes) = sample_witness();
    let circuit =
        match BatchProofCircuit::from_witness(&witness, &prices, &sizes, args.batch_size) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("from_witness: {e}");
                return ExitCode::from(2);
            }
        };
    let public_inputs = circuit.public_inputs();
    let proof_bytes = match prove(&pk, circuit, &mut rng) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("prove: {e}");
            return ExitCode::from(2);
        }
    };

    let proof = match Proof::<Bn254>::deserialize_with_mode(
        proof_bytes.0.as_slice(),
        Compress::Yes,
        Validate::Yes,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("deserialize proof: {e}");
            return ExitCode::from(2);
        }
    };

    // Sanity check: native verify before emitting JSON.
    let pvk = Groth16::<Bn254>::process_vk(&vk).expect("process_vk");
    let ok = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &public_inputs, &proof)
        .expect("verify");
    assert!(ok, "fixture proof failed Rust-side verification");

    let (ax, ay) = proof.a.xy().expect("a not infinity");
    let (bx, by) = proof.b.xy().expect("b not infinity");
    let (cx, cy) = proof.c.xy().expect("c not infinity");

    let proof_json = ProofJson {
        a: [field_hex(&ax), field_hex(&ay)],
        b: [
            // Match precompile order: [c1, c0] per Fq2 coord.
            [field_hex(&bx.c1), field_hex(&bx.c0)],
            [field_hex(&by.c1), field_hex(&by.c0)],
        ],
        c: [field_hex(&cx), field_hex(&cy)],
        public_inputs: public_inputs.iter().map(field_hex).collect(),
    };

    let pj = match serde_json::to_string_pretty(&proof_json) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("proof serialize: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = fs::write(args.out_dir.join("proof.json"), pj.as_bytes()) {
        eprintln!("write proof.json: {e}");
        return ExitCode::from(2);
    }

    eprintln!(
        "wrote {} (vk.json, proof.json)",
        args.out_dir.display()
    );
    ExitCode::SUCCESS
}
