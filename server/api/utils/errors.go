package utils

const (
	MsgMissingMetadata   = "missing metadata"
	MsgMissingAPIKey     = "missing api key"
	MsgInvalidAPIKey     = "invalid api key"
	MsgRateLimitExceeded = "rate limit exceeded"
	MsgInvalidSide       = "side must be SIDE_BUY or SIDE_SELL"
	MsgPairRequired      = "pair is required"
	MsgProofRequired      = "proof is required"
	MsgProofTooLarge      = "proof exceeds max size"
	MsgCommitmentRequired = "commitment is required"
	MsgCiphertextRequired = "encrypted_payload is required"
	MsgCiphertextTooLarge = "encrypted_payload exceeds max size"
	MsgCommitmentMismatch = "commitment does not bind encrypted_payload"
)

// MaxProofBytes caps the proof payload at the API boundary. Placeholder until the
// circuit fixes the exact proof size; keeps a malicious client from flooding the
// event log with huge blobs.
const MaxProofBytes = 64 * 1024

// MaxCiphertextBytes caps the encrypted order payload at the API boundary.
// TODO(zk-pipeline): tighten to the exact ciphertext size once the circuit
// fixes field layout and encryption scheme.
const MaxCiphertextBytes = 128 * 1024
