package core

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

const (
	DefaultAuctionInterval = 5 * time.Second
	DefaultOrderTTL        = 10 * time.Minute
	defaultSubmitTimeout   = 10 * time.Second
	defaultMinBackoff      = 1 * time.Second
	defaultMaxBackoff      = 60 * time.Second
)

type AuctionNotification struct {
	AuctionID     uuid.UUID
	Pair          string
	ClearingPrice decimal.Decimal
	MatchedVolume decimal.Decimal
	MatchCount    int
	Timestamp     time.Time
}

type Subscriber struct {
	ID string
	Ch chan AuctionNotification
}

// pendingBatch tracks a submitted-but-unconfirmed auction batch.
// submitting is set under e.mu by the first submitBatch caller so concurrent
// callers (fresh tick vs. resubmit) don't double-issue the Submit RPC.
type pendingBatch struct {
	BatchID     uuid.UUID
	AuctionID   uuid.UUID
	Matches     []event.OrderMatched
	Attempts    int
	NextAttempt time.Time
	submitting  bool
}

type Engine struct {
	mu    sync.RWMutex
	ob    *OrderBook
	store event.Store
	pairs map[string]bool

	auctionInterval time.Duration
	defaultTTL      time.Duration

	auctionLog []event.AuctionExecuted

	submitter      Submitter
	pendingBatches map[uuid.UUID]*pendingBatch
	submitTimeout  time.Duration
	minBackoff     time.Duration
	maxBackoff     time.Duration
	recovered      bool

	subMu       sync.RWMutex
	subscribers map[string]*Subscriber
}

func NewEngine(store event.Store, auctionInterval time.Duration) *Engine {
	if auctionInterval <= 0 {
		auctionInterval = DefaultAuctionInterval
	}
	return &Engine{
		ob:              NewOrderBook(),
		store:           store,
		pairs:           make(map[string]bool),
		auctionInterval: auctionInterval,
		defaultTTL:      DefaultOrderTTL,
		submitter:       NoopSubmitter{},
		pendingBatches:  make(map[uuid.UUID]*pendingBatch),
		submitTimeout:   defaultSubmitTimeout,
		minBackoff:      defaultMinBackoff,
		maxBackoff:      defaultMaxBackoff,
		subscribers:     make(map[string]*Subscriber),
	}
}

func (e *Engine) SetSubmitTimeout(d time.Duration) {
	if d <= 0 {
		d = defaultSubmitTimeout
	}
	e.mu.Lock()
	e.submitTimeout = d
	e.mu.Unlock()
}

func (e *Engine) SetRetryBackoff(min, max time.Duration) {
	if min < 0 {
		min = 0
	}
	if max < min {
		max = min
	}
	e.mu.Lock()
	e.minBackoff = min
	e.maxBackoff = max
	e.mu.Unlock()
}

func computeBackoff(min, max time.Duration, attempts int) time.Duration {
	if min <= 0 {
		return 0
	}
	if attempts < 1 {
		attempts = 1
	}
	// Go defines shifts beyond the operand width: for int64, 1<<64 == 0 and
	// 1<<63 is negative. Both cases trip the d <= 0 || d > max guard below,
	// so no separate overflow branch is needed.
	d := min << (attempts - 1)
	if d <= 0 || d > max {
		return max
	}
	return d
}

func (e *Engine) SetSubmitter(s Submitter) {
	if s == nil {
		s = NoopSubmitter{}
	}
	e.mu.Lock()
	e.submitter = s
	e.mu.Unlock()
}

func (e *Engine) Recover() error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.recovered {
		return nil
	}
	// Reset projection + derived state before the read loop. If a prior
	// Recover failed partway, ob may already have some events applied and
	// pendingBatches / auctionLog may hold stale entries. Re-applying on top
	// would double-count OrderMatched fills and corrupt RemainingSize, so
	// rebuild from scratch each attempt.
	e.ob = NewOrderBook()
	e.auctionLog = nil
	e.pendingBatches = make(map[uuid.UUID]*pendingBatch)

	const batchSize = 1024
	var after uint64
	matchesByAuction := make(map[uuid.UUID][]event.OrderMatched)

	for {
		events, err := e.store.ReadFrom(after, batchSize)
		if err != nil {
			return err
		}
		if len(events) == 0 {
			break
		}
		for _, ev := range events {
			e.ob.Apply(ev)
			switch d := ev.Data.(type) {
			case event.AuctionExecuted:
				e.auctionLog = append(e.auctionLog, d)
			case event.OrderMatched:
				matchesByAuction[d.AuctionID] = append(matchesByAuction[d.AuctionID], d)
			case event.BatchSubmitted:
				raw := matchesByAuction[d.AuctionID]
				matches := append([]event.OrderMatched(nil), raw...)
				e.pendingBatches[d.BatchID] = &pendingBatch{
					BatchID:   d.BatchID,
					AuctionID: d.AuctionID,
					Matches:   matches,
				}
				delete(matchesByAuction, d.AuctionID)
			case event.BatchConfirmed:
				delete(e.pendingBatches, d.BatchID)
			}
		}
		after = events[len(events)-1].Seq
	}
	e.recovered = true
	return nil
}

func (e *Engine) PlaceOrder(pair string, side utils.Side, price, size decimal.Decimal, commitmentKey string, ttl time.Duration) (*model.Order, error) {
	if pair == "" {
		return nil, utils.ErrPairRequired
	}
	if !price.IsPositive() {
		return nil, utils.ErrPriceMustBePositive
	}
	if !size.IsPositive() {
		return nil, utils.ErrSizeMustBePositive
	}
	if commitmentKey == "" {
		return nil, utils.ErrCommitmentKeyRequired
	}

	if ttl <= 0 {
		ttl = e.defaultTTL
	}

	now := time.Now()
	order := model.Order{
		ID:            uuid.New(),
		Pair:          pair,
		Side:          side,
		Price:         price,
		Size:          size,
		RemainingSize: size,
		CommitmentKey: commitmentKey,
		SubmittedAt:   now,
		ExpiresAt:     now.Add(ttl),
	}

	evt := event.Event{
		Type:      utils.OrderPlacedType,
		Timestamp: now,
		Data:      event.OrderPlaced{Order: order},
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	if err := e.store.Append(&evt); err != nil {
		return nil, fmt.Errorf("failed to persist order: %w", err)
	}
	e.ob.Apply(evt)
	e.pairs[pair] = true

	return &order, nil
}

func (e *Engine) CancelOrder(orderID uuid.UUID, reason string) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if !e.orderExists(orderID) {
		return fmt.Errorf("order %s: %w", orderID, utils.ErrOrderNotFound)
	}

	if reason == "" {
		reason = "user cancelled"
	}

	evt := event.Event{
		Type:      utils.OrderCancelledType,
		Timestamp: time.Now(),
		Data:      event.OrderCancelled{OrderID: orderID, Reason: reason},
	}

	if err := e.store.Append(&evt); err != nil {
		return fmt.Errorf("failed to persist cancellation: %w", err)
	}
	e.ob.Apply(evt)
	return nil
}

func (e *Engine) GetOrder(orderID uuid.UUID) *model.Order {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.ob.FindOrder(orderID)
}

func (e *Engine) GetOrderBook(pair string) (bids, asks []model.Order) {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.pairOrders(pair)
}

func (e *Engine) ActiveOrderCount() int {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.ob.ActiveOrderCount()
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
	var batchesToSubmit []uuid.UUID

	for pair := range e.pairs {
		bids, asks := e.pairOrders(pair)
		result := RunAuction(pair, bids, asks)
		if result == nil {
			continue
		}

		var events []*event.Event

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

		batchID := uuid.New()
		batchEvt := &event.Event{
			Type:      utils.BatchSubmittedType,
			Timestamp: now,
			Data: event.BatchSubmitted{
				BatchID:    batchID,
				AuctionID:  result.AuctionID,
				TxHash:     "",
				MatchCount: len(result.Matches),
			},
		}
		events = append(events, batchEvt)

		if err := e.store.Append(events...); err != nil {
			log.Printf("failed to persist auction events for pair %s: %v", pair, err)
			continue
		}
		e.auctionLog = append(e.auctionLog, auctionEvt.Data.(event.AuctionExecuted))
		for _, evt := range events {
			e.ob.Apply(*evt)
		}

		matchesCopy := append([]event.OrderMatched(nil), result.Matches...)
		e.pendingBatches[batchID] = &pendingBatch{
			BatchID:   batchID,
			AuctionID: result.AuctionID,
			Matches:   matchesCopy,
		}
		batchesToSubmit = append(batchesToSubmit, batchID)

		notifications = append(notifications, AuctionNotification{
			AuctionID:     result.AuctionID,
			Pair:          result.Pair,
			ClearingPrice: result.ClearingPrice,
			MatchedVolume: result.MatchedVolume,
			MatchCount:    len(result.Matches),
			Timestamp:     now,
		})
	}

	e.mu.Unlock()

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

	txHash, err := sub.Submit(sctx, pb.BatchID, pb.AuctionID, pb.Matches)
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

func (e *Engine) Start(ctx context.Context) {
	// resubmitPendingExcept honors NextAttempt; ResubmitPending doesn't.
	// Same behavior today since Recover leaves NextAttempt=0, but if
	// backoff ever becomes persistent this avoids hammering a failing chain
	// on crash-loops.
	e.resubmitPendingExcept(ctx, nil)

	ticker := time.NewTicker(e.auctionInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			e.RunAuctionTickCtx(ctx)
		}
	}
}

func (e *Engine) Subscribe(bufSize int) *Subscriber {
	if bufSize <= 0 {
		bufSize = 16
	}
	sub := &Subscriber{
		ID: uuid.New().String(),
		Ch: make(chan AuctionNotification, bufSize),
	}
	e.subMu.Lock()
	e.subscribers[sub.ID] = sub
	e.subMu.Unlock()
	return sub
}

func (e *Engine) Unsubscribe(id string) {
	e.subMu.Lock()
	if sub, ok := e.subscribers[id]; ok {
		close(sub.Ch)
		delete(e.subscribers, id)
	}
	e.subMu.Unlock()
}

func (e *Engine) GetAuctionHistory(pair string, limit int) ([]event.AuctionExecuted, error) {
	if limit <= 0 {
		limit = 50
	}

	e.mu.RLock()
	defer e.mu.RUnlock()

	var results []event.AuctionExecuted
	for i := len(e.auctionLog) - 1; i >= 0; i-- {
		ae := e.auctionLog[i]
		if pair == "" || ae.Pair == pair {
			results = append(results, ae)
			if len(results) >= limit {
				break
			}
		}
	}
	return results, nil
}

func (e *Engine) orderExists(orderID uuid.UUID) bool {
	return e.ob.HasOrder(orderID)
}

func (e *Engine) pairOrders(pair string) ([]model.Order, []model.Order) {
	var bids, asks []model.Order
	for _, o := range e.ob.Bids() {
		if o.Pair == pair {
			bids = append(bids, o)
		}
	}
	for _, o := range e.ob.Asks() {
		if o.Pair == pair {
			asks = append(asks, o)
		}
	}
	return bids, asks
}

func (e *Engine) notifySubscribers(n AuctionNotification) {
	e.subMu.RLock()
	defer e.subMu.RUnlock()

	for _, sub := range e.subscribers {
		select {
		case sub.Ch <- n:
		default:
			// subscriber too slow, drop notification
		}
	}
}
