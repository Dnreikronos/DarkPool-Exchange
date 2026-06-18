use dp_engine::{Engine, PairStatus};
use dp_types::{DarkPoolError, Pair};
use tonic::Status;

use crate::pb::PlaceOrderRequest;

// Bounds protect the API from oversized payloads while leaving comfortable
// slack above realistic sizes:
//   - SNARK / STARK proofs are typically a few KB; 64 KiB tolerates large
//     recursive or aggregate proofs.
//   - Encrypted order payloads (ECIES of an Order struct + serialized fields)
//     are well under 1 KB; 128 KiB tolerates future schema growth and any
//     framing overhead.
pub const MAX_PROOF_BYTES: usize = 64 * 1024;
pub const MAX_CIPHERTEXT_BYTES: usize = dp_engine::MAX_ENCRYPTED_PAYLOAD_BYTES;

// Slack above raw byte caps to absorb base64 inflation (~4/3) plus JSON
// envelope. Also caps the gRPC decode frame before prost allocates and
// decodes an oversized PlaceOrder request.
pub const PLACE_ORDER_BODY_LIMIT: usize = (MAX_PROOF_BYTES + MAX_CIPHERTEXT_BYTES) * 2;

pub const MSG_COMMITMENT_REQUIRED: &str = "commitment is required";
pub const MSG_PROOF_REQUIRED: &str = "proof is required";
pub const MSG_PROOF_TOO_LARGE: &str = "proof exceeds max size";
pub const MSG_CIPHERTEXT_REQUIRED: &str = "encrypted_payload is required";
pub const MSG_CIPHERTEXT_TOO_LARGE: &str = "encrypted_payload exceeds max size";
pub const MSG_PAIR_REQUIRED: &str = "pair is required";
pub const MSG_MISSING_API_KEY: &str = "missing api key";
pub const MSG_INVALID_API_KEY: &str = "invalid api key";
pub const MSG_MISSING_METADATA: &str = "missing metadata";
pub const MSG_RATE_LIMIT_EXCEEDED: &str = "rate limit exceeded";

/// Bounds-check the proof field before the encrypted payload reaches the
/// engine. Cryptographic verification happens after decryption in
/// `Engine::place_encrypted_order`, where the operator derives the proof's
/// commitment/nullifier public inputs from plaintext order fields.
pub fn validate_proof(proof: &[u8]) -> Result<(), Status> {
    if proof.is_empty() {
        return Err(Status::invalid_argument(MSG_PROOF_REQUIRED));
    }
    if proof.len() > MAX_PROOF_BYTES {
        return Err(Status::invalid_argument(MSG_PROOF_TOO_LARGE));
    }
    Ok(())
}

pub fn validate_place_order(req: &PlaceOrderRequest) -> Result<(), Status> {
    if req.commitment.is_empty() {
        return Err(Status::invalid_argument(MSG_COMMITMENT_REQUIRED));
    }
    validate_proof(&req.proof)?;
    if req.encrypted_payload.is_empty() {
        return Err(Status::invalid_argument(MSG_CIPHERTEXT_REQUIRED));
    }
    if req.encrypted_payload.len() > MAX_CIPHERTEXT_BYTES {
        return Err(Status::invalid_argument(MSG_CIPHERTEXT_TOO_LARGE));
    }
    Ok(())
}

/// Canonicalise the input pair; on parse failure surface an
/// `invalid_argument` status with the standard messages.
fn canonicalise_pair(pair: &str) -> Result<Pair, Status> {
    Pair::parse(pair).map_err(|e| match e {
        DarkPoolError::PairRequired => Status::invalid_argument(MSG_PAIR_REQUIRED),
        other => Status::invalid_argument(other.to_string()),
    })
}

/// Reject reads for pairs the operator has not registered (or has
/// delisted). PlaceOrder cannot use this — the pair lives inside the
/// encrypted payload — so the engine enforces the equivalent check after
/// decryption (see `Engine::place_encrypted_order`).
///
/// Returns the canonicalised [`Pair`] so handlers feed the registry key
/// (upper-case, trimmed) into downstream engine lookups instead of the raw
/// user input — otherwise `eth/usdc` would 404 against an `ETH/USDC` entry.
pub fn validate_pair_known(engine: &Engine, pair: &str) -> Result<Pair, Status> {
    let canonical = canonicalise_pair(pair)?;
    match engine.pair_status(canonical.as_str()) {
        Some(PairStatus::Active) | Some(PairStatus::Suspended) => Ok(canonical),
        Some(PairStatus::Delisted) | None => Err(Status::not_found(format!(
            "pair {} is not registered",
            canonical.as_str()
        ))),
    }
}

/// Like [`validate_pair_known`] but also accepts `Delisted`. The auction
/// log is append-only and retains records past a delist, so historical
/// reads must continue to work — delisting is forward-looking (blocks new
/// orders + new auctions), not retroactive. Unknown pairs still 404.
pub fn validate_pair_for_history(engine: &Engine, pair: &str) -> Result<Pair, Status> {
    let canonical = canonicalise_pair(pair)?;
    match engine.pair_status(canonical.as_str()) {
        Some(_) => Ok(canonical),
        None => Err(Status::not_found(format!(
            "pair {} is not registered",
            canonical.as_str()
        ))),
    }
}
