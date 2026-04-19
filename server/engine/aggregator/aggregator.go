package aggregator

import (
	"context"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/google/uuid"
)

// ProofAggregator turns a set of matched pairs into a single aggregated ZK
// proof. The default implementation (NoopAggregator) returns an empty byte
// slice so the engine can run end-to-end without a Rust toolchain installed;
// production wiring plugs an impl that shells out to the Rust aggregator CLI.
type ProofAggregator interface {
	// Aggregate is called once per batch, before the on-chain Submit. The
	// returned proof is persisted on BatchSubmitted so crash-recovery can
	// resubmit without re-running the aggregator.
	Aggregate(ctx context.Context, batchID uuid.UUID, matches []event.OrderMatched) (proof []byte, err error)
}

type NoopAggregator struct{}

func (NoopAggregator) Aggregate(_ context.Context, _ uuid.UUID, _ []event.OrderMatched) ([]byte, error) {
	return nil, nil
}
