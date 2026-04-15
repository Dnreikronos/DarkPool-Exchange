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

type Engine struct {
	mu    sync.RWMutex
	ob    *OrderBook
	store event.Store
	pairs map[string]bool // registered trading pairs

	auctionInterval time.Duration
	defaultTTL      time.Duration

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
		subscribers:     make(map[string]*Subscriber),
	}
}

// Recover rebuilds the order book from the event store.
func (e *Engine) Recover() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.ob.Replay(e.store)
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

	if err := e.store.Append(evt); err != nil {
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
		return fmt.Errorf("order %s not found", orderID)
	}

	if reason == "" {
		reason = "user cancelled"
	}

	evt := event.Event{
		Type:      utils.OrderCancelledType,
		Timestamp: time.Now(),
		Data:      event.OrderCancelled{OrderID: orderID, Reason: reason},
	}

	if err := e.store.Append(evt); err != nil {
		return fmt.Errorf("failed to persist cancellation: %w", err)
	}
	e.ob.Apply(evt)
	return nil
}

func (e *Engine) GetOrder(orderID uuid.UUID) *model.Order {
	e.mu.RLock()
	defer e.mu.RUnlock()
	o, _ := e.ob.FindOrder(orderID)
	return o
}

func (e *Engine) GetOrderBook(pair string) (bids, asks []model.Order) {
	e.mu.RLock()
	defer e.mu.RUnlock()

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

func (e *Engine) ActiveOrderCount() int {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.ob.ActiveOrderCount()
}

func (e *Engine) RunAuctionTick() []AuctionNotification {
	e.mu.Lock()

	now := time.Now()
	expiredEvents := e.ob.ExpireOrders(now)
	if len(expiredEvents) > 0 {
		if err := e.store.Append(expiredEvents...); err != nil {
			log.Printf("failed to append expired-order events: %v", err)
		}
	}

	var notifications []AuctionNotification

	for pair := range e.pairs {
		bids, asks := e.pairOrders(pair)
		result := RunAuction(pair, bids, asks)
		if result == nil {
			continue
		}

		var events []event.Event

		auctionEvt := event.Event{
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
			events = append(events, event.Event{
				Type:      utils.OrderMatchedType,
				Timestamp: now,
				Data:      m,
			})
		}

		if err := e.store.Append(events...); err != nil {
			log.Printf("failed to append auction events for pair %s: %v", pair, err)
		}
		for _, evt := range events {
			e.ob.Apply(evt)
		}

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

	for _, n := range notifications {
		e.notifySubscribers(n)
	}

	return notifications
}

func (e *Engine) Start(ctx context.Context) {
	ticker := time.NewTicker(e.auctionInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			e.RunAuctionTick()
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

	all, err := e.store.ReadFrom(0, int(e.store.LastSeq())+1)
	if err != nil {
		return nil, err
	}

	var results []event.AuctionExecuted
	for i := len(all) - 1; i >= 0; i-- {
		if ae, ok := all[i].Data.(event.AuctionExecuted); ok {
			if pair == "" || ae.Pair == pair {
				results = append(results, ae)
				if len(results) >= limit {
					break
				}
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
