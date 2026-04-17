package core

import (
	"context"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/google/uuid"
)

type Submitter interface {
	// Submit may be invoked more than once with the same BatchID after a
	// crash, timeout, or transient error. Implementations MUST be idempotent
	// keyed by BatchID so repeated calls do not produce double settlement.
	Submit(ctx context.Context, batchID uuid.UUID, auctionID uuid.UUID, matches []event.OrderMatched) (txHash string, err error)
}

type NoopSubmitter struct{}

func (NoopSubmitter) Submit(_ context.Context, batchID uuid.UUID, _ uuid.UUID, _ []event.OrderMatched) (string, error) {
	return "noop:" + batchID.String(), nil
}
