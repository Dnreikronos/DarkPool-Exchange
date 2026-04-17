package core

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

type Submitter interface {
	// Submit may be invoked more than once with the same BatchID after a
	// crash, timeout, or transient error. Implementations MUST be idempotent
	// keyed by BatchID so repeated calls do not produce double settlement.
	// proof is the aggregated proof produced by a ProofAggregator; may be nil
	// when NoopAggregator is wired.
	Submit(ctx context.Context, batchID uuid.UUID, auctionID uuid.UUID, matches []event.OrderMatched, proof []byte) (txHash string, err error)
}

type NoopSubmitter struct{}

func (NoopSubmitter) Submit(_ context.Context, batchID uuid.UUID, _ uuid.UUID, _ []event.OrderMatched, _ []byte) (string, error) {
	return "noop:" + batchID.String(), nil
}
