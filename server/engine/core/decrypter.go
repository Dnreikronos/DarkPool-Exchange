package core

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/shopspring/decimal"
)

// DecryptedOrder is the plaintext order recovered from the wire-level
// ciphertext. It exists only in engine RAM during the auction window.
type DecryptedOrder struct {
	Pair          string          `json:"pair"`
	Side          utils.Side      `json:"side"`
	Price         decimal.Decimal `json:"price"`
	Size          decimal.Decimal `json:"size"`
	CommitmentKey string          `json:"commitment_key"`
	TTL           time.Duration   `json:"ttl"`
}

// Decrypter turns an opaque wire ciphertext into a plaintext order. Real
// impls use the operator private key (ECIES / AES-GCM); NoopDecrypter is a
// JSON passthrough so the pipeline runs end-to-end without keys.
type Decrypter interface {
	Decrypt(ctx context.Context, ciphertext []byte) (DecryptedOrder, error)
}

type NoopDecrypter struct{}

// TODO(zk-pipeline): replace JSON passthrough with ECIES/AES-GCM once
// operator keypair management lands.
func (NoopDecrypter) Decrypt(_ context.Context, ct []byte) (DecryptedOrder, error) {
	var out DecryptedOrder
	if err := json.Unmarshal(ct, &out); err != nil {
		return DecryptedOrder{}, fmt.Errorf("noop decrypter: %w", err)
	}
	return out, nil
}

// CanonicalBytes produces a deterministic byte encoding used as the input to
// the commitment hash. Field ordering is fixed so the same DecryptedOrder
// always yields the same bytes.
func CanonicalBytes(o DecryptedOrder) []byte {
	return []byte(fmt.Sprintf(
		"pair=%s|side=%d|price=%s|size=%s|key=%s|ttl=%d",
		o.Pair, int(o.Side), o.Price.String(), o.Size.String(), o.CommitmentKey, int64(o.TTL),
	))
}

// ComputeCommitment is the scaffolding commitment scheme: sha256 over the
// canonical bytes. Real system uses Pedersen over the circuit field.
// TODO(zk-pipeline): replace sha256 with Pedersen once circuit lands.
func ComputeCommitment(o DecryptedOrder) []byte {
	sum := sha256.Sum256(CanonicalBytes(o))
	return sum[:]
}
