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
use dp_zk::keys::fq_to_hex;
use dp_zk::pedersen::derive_trader_id;
use dp_zk::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
use dp_zk::BatchProofCircuit;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "dp-zk-fixture-gen",
    about = "Emit Groth16 fixtures for Solidity tests"
)]
struct Args {
    #[arg(long)]
    out_dir: PathBuf,
    #[arg(long, default_value_t = 42u64)]
    seed: u64,
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

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&args.out_dir)?;

    let mut rng = StdRng::seed_from_u64(args.seed);
    let (pk, vk) = setup(args.batch_size, &mut rng)?;

    let sol_vk = dp_zk::keys::vk_to_solidity(&vk);
    let vk_json = serde_json::to_string_pretty(&sol_vk)?;
    fs::write(args.out_dir.join("vk.json"), vk_json.as_bytes())?;

    let (witness, prices, sizes) = sample_witness();
    let circuit = BatchProofCircuit::from_witness(&witness, &prices, &sizes, args.batch_size)?;
    let public_inputs = circuit.public_inputs();
    let proof_bytes = prove(&pk, circuit, &mut rng)?;

    let proof = Proof::<Bn254>::deserialize_with_mode(
        proof_bytes.0.as_slice(),
        Compress::Yes,
        Validate::Yes,
    )?;

    let pvk = Groth16::<Bn254>::process_vk(&vk).expect("process_vk");
    let ok = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &public_inputs, &proof)?;
    assert!(ok, "fixture proof failed Rust-side verification");

    let (ax, ay) = proof.a.xy().expect("a not infinity");
    let (bx, by) = proof.b.xy().expect("b not infinity");
    let (cx, cy) = proof.c.xy().expect("c not infinity");

    let proof_json = ProofJson {
        a: [fq_to_hex(&ax), fq_to_hex(&ay)],
        b: [
            [fq_to_hex(&bx.c1), fq_to_hex(&bx.c0)],
            [fq_to_hex(&by.c1), fq_to_hex(&by.c0)],
        ],
        c: [fq_to_hex(&cx), fq_to_hex(&cy)],
        public_inputs: public_inputs.iter().map(fq_to_hex).collect(),
    };

    let pj = serde_json::to_string_pretty(&proof_json)?;
    fs::write(args.out_dir.join("proof.json"), pj.as_bytes())?;

    eprintln!("wrote {} (vk.json, proof.json)", args.out_dir.display());
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
