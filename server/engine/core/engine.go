package core

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"sync"
	"time"

	"github.com/darkpool-exchange/server/engine/aggregator"
	"github.com/darkpool-exchange/server/engine/book"
	"github.com/darkpool-exchange/server/engine/decrypt"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/darkpool-exchange/server/engine/settlement"
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
	ob    *book.OrderBook
	store event.Store
	pairs map[string]bool

	auctionInterval time.Duration
	defaultTTL      time.Duration

	auctionLog []event.AuctionExecuted

	aggregator     aggregator.ProofAggregator
	submitter      settlement.Submitter
	decrypter      decrypt.Decrypter
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
		ob:              book.New(),
		store:           store,
		pairs:           make(map[string]bool),
		auctionInterval: auctionInterval,
		defaultTTL:      DefaultOrderTTL,
		aggregator:      aggregator.NoopAggregator{},
		submitter:       settlement.NoopSubmitter{},
		decrypter:       decrypt.NoopDecrypter{},
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

func (e *Engine) SetSubmitter(s settlement.Submitter) {
	if s == nil {
		s = settlement.NoopSubmitter{}
	}
	e.mu.Lock()
	e.submitter = s
	e.mu.Unlock()
}

func (e *Engine) SetDecrypter(d decrypt.Decrypter) {
	if d == nil {
		d = decrypt.NoopDecrypter{}
	}
	e.mu.Lock()
	e.decrypter = d
	e.mu.Unlock()
}

func (e *Engine) SetAggregator(a aggregator.ProofAggregator) {
	if a == nil {
		a = aggregator.NoopAggregator{}
	}
	e.mu.Lock()
	e.aggregator = a
	e.mu.Unlock()
}

func (e *Engine) Recover(ctx context.Context) error {
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
	e.ob = book.New()
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
				decrypted, err := e.decrypter.Decrypt(ctx, d.Ciphertext)
				if err != nil {
					return fmt.Errorf("recover: decrypt order %s: %w", d.OrderID, err)
				}
				if !bytes.Equal(decrypt.ComputeCommitment(decrypted), d.Commitment) {
					return fmt.Errorf("recover: %w for order %s", utils.ErrCommitmentMismatch, d.OrderID)
				}
				ttl := decrypted.TTL
				if ttl <= 0 {
					ttl = e.defaultTTL
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
					SubmittedAt:      ev.Timestamp,
					ExpiresAt:        ev.Timestamp.Add(ttl),
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
			case event.BatchSettled:
				delete(e.pendingBatches, d.BatchID)
			}
		}
		after = events[len(events)-1].Seq
	}

	// Any AuctionID still sitting in matchesByAuction has OrderMatched events
	// persisted without a trailing BatchSubmitted — a crash between the two
	// Append calls in RunAuctionTickCtx. Re-run Aggregate for each orphan so
	// the durable log and pendingBatches converge with pre-crash intent.
	//
	// Iterate in sorted AuctionID order so two replays of the same store emit
	// identical BatchSubmitted sequences (batchIDs differ — they're freshly
	// generated — but match assignments per auction stay deterministic).
	auctionTimestamps := make(map[uuid.UUID]time.Time, len(e.auctionLog))
	for _, ae := range e.auctionLog {
		auctionTimestamps[ae.AuctionID] = ae.Timestamp
	}
	orphanIDs := make([]uuid.UUID, 0, len(matchesByAuction))
	for id := range matchesByAuction {
		orphanIDs = append(orphanIDs, id)
	}
	sort.Slice(orphanIDs, func(i, j int) bool {
		return orphanIDs[i].String() < orphanIDs[j].String()
	})
	for _, auctionID := range orphanIDs {
		matches := matchesByAuction[auctionID]
		batchID := uuid.New()
		proof, err := e.aggregator.Aggregate(ctx, batchID, matches)
		if err != nil {
			return fmt.Errorf("recover: re-aggregate orphan auction %s: %w", auctionID, err)
		}
		ts, ok := auctionTimestamps[auctionID]
		if !ok {
			// Shouldn't happen: AuctionExecuted is persisted in the same
			// Append call as OrderMatched. Fall back so recovery still makes
			// forward progress rather than failing the whole boot.
			ts = time.Now()
		}
		batchEvt := event.Event{
			Type:      utils.BatchSubmittedType,
			Timestamp: ts,
			Data: event.BatchSubmitted{
				BatchID:    batchID,
				AuctionID:  auctionID,
				TxHash:     "",
				MatchCount: len(matches),
				Proof:      append([]byte(nil), proof...),
			},
		}
		if err := e.store.Append(&batchEvt); err != nil {
			return fmt.Errorf("recover: persist re-aggregated batch for auction %s: %w", auctionID, err)
		}
		e.ob.Apply(batchEvt)
		e.pendingBatches[batchID] = &pendingBatch{
			BatchID:   batchID,
			AuctionID: auctionID,
			Matches:   append([]event.OrderMatched(nil), matches...),
			Proof:     append([]byte(nil), proof...),
		}
	}

	e.recovered = true
	return nil
}

// placeOrderPlaintext is a test-only shortcut: it packs the plaintext fields
// into a JSON ciphertext the NoopDecrypter can reverse, then runs the normal
// encrypt-only persistence path. Production uses PlaceEncryptedOrder.
func (e *Engine) placeOrderPlaintext(pair string, side utils.Side, price, size decimal.Decimal, commitmentKey string, ttl time.Duration) (*model.Order, error) {
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

	d := decrypt.DecryptedOrder{
		Pair: pair, Side: side, Price: price, Size: size,
		CommitmentKey: commitmentKey, TTL: ttl,
	}
	ct, err := json.Marshal(d)
	if err != nil {
		return nil, fmt.Errorf("encode synthetic ciphertext: %w", err)
	}
	return e.PlaceEncryptedOrder(context.Background(), decrypt.ComputeCommitment(d), nil, ct)
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

	if !bytes.Equal(decrypt.ComputeCommitment(decrypted), commitment) {
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
			OrderID:    order.ID,
			Commitment: append([]byte(nil), commitment...),
			Proof:      append([]byte(nil), proof...),
			Ciphertext: append([]byte(nil), ciphertext...),
		},
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	if err := e.store.Append(&evt); err != nil {
		return nil, fmt.Errorf("failed to persist order: %w", err)
	}
	// Apply advances ob.seq so Replay after a snapshot doesn't re-read
	// already-processed OrderPlaced events. The apply branch for OrderPlaced is
	// a no-op otherwise — the book insert happens below, from the decrypted
	// order.
	e.ob.Apply(evt)
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
