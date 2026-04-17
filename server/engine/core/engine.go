package core

import (
	"bytes"
	"context"
	"encoding/json"
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
	Proof       []byte
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

	aggregator     ProofAggregator
	submitter      Submitter
	decrypter      Decrypter
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
		aggregator:      NoopAggregator{},
		submitter:       NoopSubmitter{},
		decrypter:       NoopDecrypter{},
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

func (e *Engine) SetDecrypter(d Decrypter) {
	if d == nil {
		d = NoopDecrypter{}
	}
	e.mu.Lock()
	e.decrypter = d
	e.mu.Unlock()
}

func (e *Engine) SetAggregator(a ProofAggregator) {
	if a == nil {
		a = NoopAggregator{}
	}
	e.mu.Lock()
	e.aggregator = a
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
	e.pairs = make(map[string]bool)

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
			case event.OrderPlaced:
				// Recovery cost now scales with one decrypt per replayed
				// order. Acceptable for scaffolding — batched decrypt or a
				// plaintext-in-RAM snapshot is a later optimization.
				decrypted, err := e.decrypter.Decrypt(context.Background(), d.Ciphertext)
				if err != nil {
					return fmt.Errorf("recover: decrypt order %s: %w", d.OrderID, err)
				}
				if !bytes.Equal(ComputeCommitment(decrypted), d.Commitment) {
					return fmt.Errorf("recover: %w for order %s", utils.ErrCommitmentMismatch, d.OrderID)
				}
				order := model.Order{
					ID:               d.OrderID,
					Pair:             decrypted.Pair,
					Side:             decrypted.Side,
					Price:            decrypted.Price,
					Size:             decrypted.Size,
					RemainingSize:    decrypted.Size,
					CommitmentKey:    decrypted.CommitmentKey,
					EncryptedPayload: d.Ciphertext,
					SubmittedAt:      d.SubmittedAt,
					ExpiresAt:        d.ExpiresAt,
				}
				e.ob.InsertOrder(&order)
				e.pairs[order.Pair] = true
			case event.AuctionExecuted:
				e.auctionLog = append(e.auctionLog, d)
			case event.OrderMatched:
				matchesByAuction[d.AuctionID] = append(matchesByAuction[d.AuctionID], d)
			case event.BatchSubmitted:
				raw := matchesByAuction[d.AuctionID]
				matches := append([]event.OrderMatched(nil), raw...)
				// Proof is copied from the persisted event so resubmit reuses
				// the original aggregated proof rather than re-running the
				// aggregator (non-deterministic across toolchain versions).
				e.pendingBatches[d.BatchID] = &pendingBatch{
					BatchID:   d.BatchID,
					AuctionID: d.AuctionID,
					Matches:   matches,
					Proof:     append([]byte(nil), d.Proof...),
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

// PlaceOrder is kept as an internal helper for engine and batch tests that
// prefer to construct orders from plaintext fields. It funnels through the
// same encrypt-only persistence path by synthesizing a JSON ciphertext that
// the NoopDecrypter can reverse on Recover.
// TODO(zk-pipeline): remove once tests adopt the encrypted entrypoint.
func (e *Engine) PlaceOrder(pair string, side utils.Side, price, size decimal.Decimal, commitmentKey string, ttl time.Duration, _ []byte) (*model.Order, error) {
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

	d := DecryptedOrder{
		Pair: pair, Side: side, Price: price, Size: size,
		CommitmentKey: commitmentKey, TTL: ttl,
	}
	ct, err := json.Marshal(d)
	if err != nil {
		return nil, fmt.Errorf("encode synthetic ciphertext: %w", err)
	}
	return e.PlaceEncryptedOrder(context.Background(), ComputeCommitment(d), nil, ct)
}

// PlaceEncryptedOrder accepts the wire-level {commitment, proof, ciphertext}
// tuple. The decrypter recovers the plaintext order; the commitment must bind
// the decrypted fields or the call is rejected. This prevents a client from
// sending a proof for order A together with ciphertext/commitment of order B.
func (e *Engine) PlaceEncryptedOrder(ctx context.Context, commitment, proof, ciphertext []byte) (*model.Order, error) {
	e.mu.RLock()
	d := e.decrypter
	e.mu.RUnlock()

	decrypted, err := d.Decrypt(ctx, ciphertext)
	if err != nil {
		return nil, fmt.Errorf("decrypt order: %w", err)
	}

	if !bytes.Equal(ComputeCommitment(decrypted), commitment) {
		return nil, utils.ErrCommitmentMismatch
	}

	if decrypted.Pair == "" {
		return nil, utils.ErrPairRequired
	}
	if !decrypted.Price.IsPositive() {
		return nil, utils.ErrPriceMustBePositive
	}
	if !decrypted.Size.IsPositive() {
		return nil, utils.ErrSizeMustBePositive
	}
	if decrypted.CommitmentKey == "" {
		return nil, utils.ErrCommitmentKeyRequired
	}

	ttl := decrypted.TTL
	if ttl <= 0 {
		ttl = e.defaultTTL
	}

	order := buildOrder(decrypted.Pair, decrypted.Side, decrypted.Price, decrypted.Size, decrypted.CommitmentKey, ttl, ciphertext)
	return e.persistOrderPlaced(order, commitment, proof, ciphertext)
}

func buildOrder(pair string, side utils.Side, price, size decimal.Decimal, commitmentKey string, ttl time.Duration, encryptedPayload []byte) model.Order {
	now := time.Now()
	return model.Order{
		ID:               uuid.New(),
		Pair:             pair,
		Side:             side,
		Price:            price,
		Size:             size,
		RemainingSize:    size,
		CommitmentKey:    commitmentKey,
		EncryptedPayload: encryptedPayload,
		SubmittedAt:      now,
		ExpiresAt:        now.Add(ttl),
	}
}

func (e *Engine) persistOrderPlaced(order model.Order, commitment, proof, ciphertext []byte) (*model.Order, error) {
	evt := event.Event{
		Type:      utils.OrderPlacedType,
		Timestamp: order.SubmittedAt,
		Data: event.OrderPlaced{
			OrderID:     order.ID,
			Commitment:  append([]byte(nil), commitment...),
			Proof:       append([]byte(nil), proof...),
			Ciphertext:  append([]byte(nil), ciphertext...),
			SubmittedAt: order.SubmittedAt,
			ExpiresAt:   order.ExpiresAt,
		},
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	if err := e.store.Append(&evt); err != nil {
		return nil, fmt.Errorf("failed to persist order: %w", err)
	}
	// Plaintext never enters the event log; engine inserts directly into the
	// in-memory book. Recover rebuilds by decrypting ciphertext events.
	e.ob.InsertOrder(&order)
	e.pairs[order.Pair] = true

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

		// Aggregate before persisting BatchSubmitted so the proof is durable
		// with the batch record. Recovery reuses this exact proof on resubmit.
		// NoopAggregator returns nil instantly; a real aggregator may block,
		// which stalls fresh orders because e.mu is held. Acceptable during
		// the scaffolding phase; move to async once the Rust CLI lands.
		// TODO(zk-pipeline): move Aggregate off the engine mutex before wiring
		// the Rust aggregator CLI — see https://github.com/Dnreikronos/DarkPool-Exchange/issues/new
		proof, err := e.aggregator.Aggregate(ctx, batchID, result.Matches)
		if err != nil {
			log.Printf("aggregator failed for batch %s on pair %s: %v", batchID, pair, err)
			continue
		}

		batchEvt := &event.Event{
			Type:      utils.BatchSubmittedType,
			Timestamp: now,
			Data: event.BatchSubmitted{
				BatchID:    batchID,
				AuctionID:  result.AuctionID,
				TxHash:     "",
				MatchCount: len(result.Matches),
				Proof:      proof,
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
			Proof:     proof,
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
