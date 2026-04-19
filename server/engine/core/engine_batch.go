package core

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/darkpool-exchange/server/engine/auction"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
)

// pendingAggregation holds per-pair auction results persisted before
// aggregation. Aggregate runs off e.mu using this struct's inputs; the proof
// is persisted back into a BatchSubmitted event afterwards.
type pendingAggregation struct {
	BatchID   uuid.UUID
	AuctionID uuid.UUID
	Pair      string
	Matches   []event.OrderMatched
	AuctionAt time.Time
}

func (e *Engine) RunAuctionTickCtx(ctx context.Context) []AuctionNotification {
	e.mu.Lock()

	now := time.Now()
	expiredEvents := e.ob.CollectExpired(now)
	if len(expiredEvents) > 0 {
		if err := e.store.Append(expiredEvents...); err != nil {
			log.Printf("failed to persist expiry events: %v", err)
		} else {
			for _, evt := range expiredEvents {
				e.ob.Apply(*evt)
			}
		}
	}

	var notifications []AuctionNotification
	var pending []pendingAggregation

	for pair := range e.pairs {
		bids, asks := e.pairOrders(pair)
		result := auction.Run(pair, bids, asks)
		if result == nil {
			continue
		}

		events := make([]*event.Event, 0, 1+len(result.Matches))

		auctionEvt := &event.Event{
			Type:      utils.AuctionExecutedType,
			Timestamp: now,
			Data: event.AuctionExecuted{
				AuctionID:     result.AuctionID,
				Pair:          result.Pair,
				ClearingPrice: result.ClearingPrice,
				MatchedVolume: result.MatchedVolume,
				MatchCount:    len(result.Matches),
				Timestamp:     now,
			},
		}
		events = append(events, auctionEvt)

		for _, m := range result.Matches {
			events = append(events, &event.Event{
				Type:      utils.OrderMatchedType,
				Timestamp: now,
				Data:      m,
			})
		}

		// Persist AuctionExecuted + OrderMatched now, under the lock, so the
		// orderbook projection and the durable log stay in sync. BatchSubmitted
		// is deferred until after Aggregate runs off-lock; recovery handles
		// the gap via reAggregateOrphans.
		if err := e.store.Append(events...); err != nil {
			log.Printf("failed to persist auction events for pair %s: %v", pair, err)
			continue
		}
		e.auctionLog = append(e.auctionLog, auctionEvt.Data.(event.AuctionExecuted))
		for _, evt := range events {
			e.ob.Apply(*evt)
		}

		notif := AuctionNotification{
			AuctionID:     result.AuctionID,
			Pair:          result.Pair,
			ClearingPrice: result.ClearingPrice,
			MatchedVolume: result.MatchedVolume,
			MatchCount:    len(result.Matches),
			Timestamp:     now,
		}
		notifications = append(notifications, notif)

		pending = append(pending, pendingAggregation{
			BatchID:   uuid.New(),
			AuctionID: result.AuctionID,
			Pair:      result.Pair,
			Matches:   append([]event.OrderMatched(nil), result.Matches...),
			AuctionAt: now,
		})
	}

	agg := e.aggregator
	e.mu.Unlock()

	// Aggregate + persist BatchSubmitted OFF the engine lock so a slow
	// aggregator does not stall PlaceOrder / CancelOrder. Ordering within a
	// tick is preserved by processing pending entries sequentially.
	batchesToSubmit := make([]uuid.UUID, 0, len(pending))
	for _, p := range pending {
		proof, err := agg.Aggregate(ctx, p.BatchID, p.Matches)
		if err != nil {
			log.Printf("aggregator failed for batch %s on pair %s: %v", p.BatchID, p.Pair, err)
			continue
		}
		if err := e.finalizePendingBatch(p, proof); err != nil {
			log.Printf("finalize batch %s: %v", p.BatchID, err)
			continue
		}
		batchesToSubmit = append(batchesToSubmit, p.BatchID)
	}

	// Notify before submit: don't stall subscribers behind submitTimeout ×
	// pending batches.
	for _, n := range notifications {
		e.notifySubscribers(n)
	}

	submitSet := make(map[uuid.UUID]struct{}, len(batchesToSubmit))
	for _, id := range batchesToSubmit {
		submitSet[id] = struct{}{}
		if err := e.submitBatch(ctx, id); err != nil {
			log.Printf("batch %s submit failed, will retry next tick: %v", id, err)
		}
	}

	e.resubmitPendingExcept(ctx, submitSet)

	return notifications
}

// OnBatchSettled is the settlement.Watcher hook: apply the BatchSettled
// event to the orderbook projection (advance seq) and drop any still-pending
// batch entry. Called after the event is already persisted, so failure here
// is a soft error — pendingBatches may hold a stale entry but the durable
// log is authoritative.
func (e *Engine) OnBatchSettled(evt event.Event, batchID uuid.UUID) {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.ob.Apply(evt)
	delete(e.pendingBatches, batchID)
}

// finalizePendingBatch persists BatchSubmitted with the produced proof, applies
// it to the orderbook projection, and registers the batch for submission.
// Called OFF the engine lock, which it re-acquires only for the mutations.
func (e *Engine) finalizePendingBatch(p pendingAggregation, proof []byte) error {
	batchEvt := event.Event{
		Type:      utils.BatchSubmittedType,
		Timestamp: p.AuctionAt,
		Data: event.BatchSubmitted{
			BatchID:    p.BatchID,
			AuctionID:  p.AuctionID,
			TxHash:     "",
			MatchCount: len(p.Matches),
			Proof:      append([]byte(nil), proof...),
		},
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	if err := e.store.Append(&batchEvt); err != nil {
		return fmt.Errorf("persist BatchSubmitted: %w", err)
	}
	e.ob.Apply(batchEvt)
	e.pendingBatches[p.BatchID] = &pendingBatch{
		BatchID:   p.BatchID,
		AuctionID: p.AuctionID,
		Matches:   append([]event.OrderMatched(nil), p.Matches...),
		Proof:     append([]byte(nil), proof...),
	}
	return nil
}

func (e *Engine) resubmitPendingExcept(ctx context.Context, skip map[uuid.UUID]struct{}) {
	now := time.Now()
	e.mu.RLock()
	ids := make([]uuid.UUID, 0, len(e.pendingBatches))
	for id, pb := range e.pendingBatches {
		if _, ok := skip[id]; ok {
			continue
		}
		if now.Before(pb.NextAttempt) {
			continue
		}
		ids = append(ids, id)
	}
	e.mu.RUnlock()

	for _, id := range ids {
		if err := e.submitBatch(ctx, id); err != nil {
			log.Printf("retry batch %s: %v", id, err)
		}
	}
}

func (e *Engine) noteSubmitFailure(batchID uuid.UUID) {
	e.mu.Lock()
	defer e.mu.Unlock()
	pb, ok := e.pendingBatches[batchID]
	if !ok {
		return
	}
	pb.Attempts++
	pb.NextAttempt = time.Now().Add(computeBackoff(e.minBackoff, e.maxBackoff, pb.Attempts))
	pb.submitting = false
}

func (e *Engine) submitBatch(ctx context.Context, batchID uuid.UUID) error {
	e.mu.Lock()
	pb, ok := e.pendingBatches[batchID]
	if !ok {
		e.mu.Unlock()
		return nil
	}
	if pb.submitting {
		// Another goroutine already owns the in-flight Submit for this batch.
		// Bailing keeps us to exactly one Submit RPC per batch per attempt.
		e.mu.Unlock()
		return nil
	}
	pb.submitting = true
	sub := e.submitter
	timeout := e.submitTimeout
	e.mu.Unlock()

	// Panic would skip both noteSubmitFailure and the success-path delete,
	// leaving submitting=true and wedging the batch forever.
	defer func() {
		r := recover()
		if r == nil {
			return
		}
		e.mu.Lock()
		if pb, ok := e.pendingBatches[batchID]; ok && pb.submitting {
			pb.submitting = false
		}
		e.mu.Unlock()
		panic(r)
	}()

	sctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	txHash, err := sub.Submit(sctx, pb.BatchID, pb.AuctionID, pb.Matches, pb.Proof)
	if err != nil {
		e.noteSubmitFailure(batchID)
		return err
	}

	confirmed := event.Event{
		Type:      utils.BatchConfirmedType,
		Timestamp: time.Now(),
		Data: event.BatchConfirmed{
			BatchID: pb.BatchID,
			TxHash:  txHash,
		},
	}

	// Append outside e.mu so FileStore's fsync doesn't stall PlaceOrder /
	// CancelOrder / RunAuctionTickCtx. submitting=true still blocks any
	// other submitBatch goroutine from racing us on this BatchID.
	if err := e.store.Append(&confirmed); err != nil {
		// Submit landed on-chain but we couldn't record confirm. Bump
		// Attempts/NextAttempt so retry respects backoff (idempotent Submit
		// contract covers the double RPC).
		e.noteSubmitFailure(batchID)
		return fmt.Errorf("persist batch confirmation: %w", err)
	}

	// Entry guaranteed present: submitting=true blocks other submitBatch,
	// PlaceOrder/CancelOrder don't touch pendingBatches, Recover only runs
	// pre-submit.
	e.mu.Lock()
	defer e.mu.Unlock()
	e.ob.Apply(confirmed)
	delete(e.pendingBatches, batchID)
	return nil
}

func (e *Engine) ResubmitPending(ctx context.Context) {
	e.mu.RLock()
	ids := make([]uuid.UUID, 0, len(e.pendingBatches))
	for id := range e.pendingBatches {
		ids = append(ids, id)
	}
	e.mu.RUnlock()

	for _, id := range ids {
		if err := e.submitBatch(ctx, id); err != nil {
			log.Printf("resubmit batch %s: %v", id, err)
		}
	}
}

func (e *Engine) PendingBatchCount() int {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return len(e.pendingBatches)
}
