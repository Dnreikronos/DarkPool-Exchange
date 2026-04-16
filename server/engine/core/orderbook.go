package core

import (
	"sort"
	"sync"
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/google/uuid"
)

// OrderBook is a projection rebuilt from the event stream.
type OrderBook struct {
	mu   sync.RWMutex
	bids map[uuid.UUID]*model.Order
	asks map[uuid.UUID]*model.Order
	seq  uint64
}

func NewOrderBook() *OrderBook {
	return &OrderBook{
		bids: make(map[uuid.UUID]*model.Order),
		asks: make(map[uuid.UUID]*model.Order),
	}
}

func (ob *OrderBook) Replay(store event.Store) error {
	const batchSize = 1024

	ob.mu.Lock()
	defer ob.mu.Unlock()

	after := ob.seq

	for {
		events, err := store.ReadFrom(after, batchSize)
		if err != nil {
			return err
		}
		if len(events) == 0 {
			break
		}
		for _, e := range events {
			ob.apply(e)
		}
		after = events[len(events)-1].Seq
	}
	return nil
}

// Apply applies a single event (thread-safe).
func (ob *OrderBook) Apply(e event.Event) {
	ob.mu.Lock()
	defer ob.mu.Unlock()
	ob.apply(e)
}

func (ob *OrderBook) apply(e event.Event) {
	ob.seq = e.Seq

	switch d := e.Data.(type) {
	case event.OrderPlaced:
		o := d.Order
		switch o.Side {
		case utils.Buy:
			ob.bids[o.ID] = &o
		case utils.Sell:
			ob.asks[o.ID] = &o
		}

	case event.OrderCancelled:
		delete(ob.bids, d.OrderID)
		delete(ob.asks, d.OrderID)

	case event.OrderExpired:
		delete(ob.bids, d.OrderID)
		delete(ob.asks, d.OrderID)

	case event.OrderMatched:
		ob.applyFill(d.Bid)
		ob.applyFill(d.Ask)
	}
}

func (ob *OrderBook) applyFill(f model.Fill) {
	if o, ok := ob.bids[f.OrderID]; ok {
		o.RemainingSize = o.RemainingSize.Sub(f.Size)
		if !o.RemainingSize.IsPositive() {
			delete(ob.bids, f.OrderID)
		}
		return
	}
	if o, ok := ob.asks[f.OrderID]; ok {
		o.RemainingSize = o.RemainingSize.Sub(f.Size)
		if !o.RemainingSize.IsPositive() {
			delete(ob.asks, f.OrderID)
		}
	}
}

func (ob *OrderBook) Bids() []model.Order {
	ob.mu.RLock()
	defer ob.mu.RUnlock()
	out := make([]model.Order, 0, len(ob.bids))
	for _, o := range ob.bids {
		out = append(out, *o)
	}
	sort.Slice(out, func(i, j int) bool {
		if !out[i].Price.Equal(out[j].Price) {
			return out[i].Price.GreaterThan(out[j].Price)
		}
		return out[i].SubmittedAt.Before(out[j].SubmittedAt)
	})
	return out
}

func (ob *OrderBook) Asks() []model.Order {
	ob.mu.RLock()
	defer ob.mu.RUnlock()
	out := make([]model.Order, 0, len(ob.asks))
	for _, o := range ob.asks {
		out = append(out, *o)
	}
	sort.Slice(out, func(i, j int) bool {
		if !out[i].Price.Equal(out[j].Price) {
			return out[i].Price.LessThan(out[j].Price)
		}
		return out[i].SubmittedAt.Before(out[j].SubmittedAt)
	})
	return out
}

// CollectExpired returns expiry events for orders past their TTL without
// mutating the orderbook. The caller must Apply them after successful persistence.
func (ob *OrderBook) CollectExpired(now time.Time) []event.Event {
	ob.mu.RLock()
	defer ob.mu.RUnlock()

	var expired []event.Event
	for _, o := range ob.bids {
		if !o.ExpiresAt.IsZero() && now.After(o.ExpiresAt) {
			expired = append(expired, event.Event{
				Type: utils.OrderExpiredType,
				Data: event.OrderExpired{OrderID: o.ID},
			})
		}
	}
	for _, o := range ob.asks {
		if !o.ExpiresAt.IsZero() && now.After(o.ExpiresAt) {
			expired = append(expired, event.Event{
				Type: utils.OrderExpiredType,
				Data: event.OrderExpired{OrderID: o.ID},
			})
		}
	}
	return expired
}

func (ob *OrderBook) FindOrder(id uuid.UUID) *model.Order {
	ob.mu.RLock()
	defer ob.mu.RUnlock()
	if o, ok := ob.bids[id]; ok {
		cp := *o
		return &cp
	}
	if o, ok := ob.asks[id]; ok {
		cp := *o
		return &cp
	}
	return nil
}

func (ob *OrderBook) HasOrder(id uuid.UUID) bool {
	ob.mu.RLock()
	defer ob.mu.RUnlock()
	if _, ok := ob.bids[id]; ok {
		return true
	}
	_, ok := ob.asks[id]
	return ok
}

func (ob *OrderBook) ActiveOrderCount() int {
	ob.mu.RLock()
	defer ob.mu.RUnlock()
	return len(ob.bids) + len(ob.asks)
}
