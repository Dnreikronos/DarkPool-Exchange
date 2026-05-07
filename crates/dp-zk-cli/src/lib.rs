//! Helpers shared by the dp-zk-cli + dp-zk-keygen binaries.

use std::path::PathBuf;

use dp_zk::witness::BatchWitness;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

/// Wire-format input matching `SubprocessAggregator`'s extended schema.
#[derive(Debug, Deserialize)]
pub struct AggregatorInput {
    pub batch_id: String,
    pub matches: Vec<AggregatorMatch>,
    pub private_witness: Option<Vec<dp_zk::witness::MatchWitness>>,
    pub policy: Option<dp_zk::witness::Policy>,
}

#[derive(Debug, Deserialize)]
pub struct AggregatorMatch {
    pub auction_id: String,
    pub bid_order_id: String,
    pub ask_order_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
}

/// Resolve the keys directory from CLI flag → env var → manifest default.
pub fn resolve_keys_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(p) = flag {
        return p;
    }
    if let Ok(env) = std::env::var("DARKPOOL_ZK_PROVING_KEY") {
        if !env.is_empty() {
            return PathBuf::from(env);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dp-zk/keys")
}

/// Output of [`build_witness`]: parsed batch metadata, per-match
/// (price, size) pulled directly off the wire, and the full
/// [`BatchWitness`] ready to feed [`dp_zk::BatchProofCircuit`].
pub struct ParsedInput {
    pub batch_id: Uuid,
    pub auction_ids: Vec<Uuid>,
    pub witness: BatchWitness,
    pub prices: Vec<Decimal>,
    pub sizes: Vec<Decimal>,
}

/// Build a [`BatchWitness`] from parsed CLI input, falling back to the
/// default policy when omitted. Rejects empty batches and witnesses whose
/// length disagrees with the public matches array — those would either
/// burn a guaranteed-failing prove cycle or silently coerce to a `nil`
/// auction id.
pub fn build_witness(input: AggregatorInput) -> Result<ParsedInput, String> {
    let batch_id = Uuid::parse_str(&input.batch_id).map_err(|e| format!("batch_id: {e}"))?;
    if input.matches.is_empty() {
        return Err("matches array is empty".into());
    }
    let auction_ids: Vec<Uuid> = input
        .matches
        .iter()
        .map(|m| Uuid::parse_str(&m.auction_id).map_err(|e| format!("auction_id: {e}")))
        .collect::<Result<_, _>>()?;
    let auction_id = auction_ids[0];
    if auction_ids.iter().any(|a| *a != auction_id) {
        return Err("matches span multiple auction ids".into());
    }
    let private = input
        .private_witness
        .ok_or_else(|| "missing private_witness; not invoked via dp-zk integration".to_string())?;
    if private.len() != input.matches.len() {
        return Err(format!(
            "matches/private_witness length mismatch: matches={}, private_witness={}",
            input.matches.len(),
            private.len()
        ));
    }
    let prices: Vec<Decimal> = input.matches.iter().map(|m| m.price).collect();
    let sizes: Vec<Decimal> = input.matches.iter().map(|m| m.size).collect();
    let policy = input
        .policy
        .unwrap_or_else(|| dp_zk::witness::DEFAULT_POLICY.into_policy());
    let witness = BatchWitness {
        batch_id,
        auction_id,
        matches: private,
        policy,
    };
    Ok(ParsedInput {
        batch_id,
        auction_ids,
        witness,
        prices,
        sizes,
    })
}
