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
pub const MAX_CIPHERTEXT_BYTES: usize = 128 * 1024;

pub const MSG_COMMITMENT_REQUIRED: &str = "commitment is required";
pub const MSG_PROOF_REQUIRED: &str = "proof is required";
pub const MSG_PROOF_TOO_LARGE: &str = "proof exceeds max size";
pub const MSG_CIPHERTEXT_REQUIRED: &str = "encrypted_payload is required";
pub const MSG_CIPHERTEXT_TOO_LARGE: &str = "encrypted_payload exceeds max size";
pub const MSG_COMMITMENT_MISMATCH: &str = "commitment does not bind encrypted_payload";
pub const MSG_PAIR_REQUIRED: &str = "pair is required";
pub const MSG_MISSING_API_KEY: &str = "missing api key";
pub const MSG_INVALID_API_KEY: &str = "invalid api key";
pub const MSG_MISSING_METADATA: &str = "missing metadata";
pub const MSG_RATE_LIMIT_EXCEEDED: &str = "rate limit exceeded";

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
