package engine

import (
	"testing"
	"time"

	"github.com/darkpool-exchange/engine/utils"
	"github.com/darkpool-exchange/engine/event"
	"github.com/darkpool-exchange/engine/model"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

func newOrder(side utils.Side, price, size int64) model.Order {
	return model.Order{
		ID:            uuid.New(),
		Pair:          "ETH/USDC",
		Side:          side,
		Price:         decimal.NewFromInt(price),
		Size:          decimal.NewFromInt(size),
		RemainingSize: decimal.NewFromInt(size),
		CommitmentKey: uuid.NewString(),
		SubmittedAt:   time.Now(),
		ExpiresAt:     time.Now().Add(10 * time.Minute),
	}
}

func TestOrderBook_PlaceAndCancel(t *testing.T) {
	ob := NewOrderBook()
	o := newOrder(utils.Buy, 1800, 10)

	ob.Apply(event.Event{Seq: 1, Type: utils.OrderPlacedType, Data: event.OrderPlaced{Order: o}})

	if got := ob.ActiveOrderCount(); got != 1 {
		t.Fatalf("ActiveOrderCount = %d, want 1", got)
	}

	ob.Apply(event.Event{Seq: 2, Type: utils.OrderCancelledType, Data: event.OrderCancelled{OrderID: o.ID}})

	if got := ob.ActiveOrderCount(); got != 0 {
		t.Fatalf("ActiveOrderCount after cancel = %d, want 0", got)
	}
}

func TestOrderBook_PartialFill(t *testing.T) {
	ob := NewOrderBook()
	bid := newOrder(utils.Buy, 1800, 10)
	ask := newOrder(utils.Sell, 1790, 4)

	ob.Apply(event.Event{Seq: 1, Type: utils.OrderPlacedType, Data: event.OrderPlaced{Order: bid}})
	ob.Apply(event.Event{Seq: 2, Type: utils.OrderPlacedType, Data: event.OrderPlaced{Order: ask}})

	ob.Apply(event.Event{Seq: 3, Type: utils.OrderMatchedType, Data: event.OrderMatched{
		Bid:   model.Fill{OrderID: bid.ID, Size: decimal.NewFromInt(4)},
		Ask:   model.Fill{OrderID: ask.ID, Size: decimal.NewFromInt(4)},
		Price: decimal.NewFromInt(1795),
		Size:  decimal.NewFromInt(4),
	}})

	bids := ob.Bids()
	asks := ob.Asks()

	if len(asks) != 0 {
		t.Fatalf("asks count = %d, want 0", len(asks))
	}
	if len(bids) != 1 {
		t.Fatalf("bids count = %d, want 1", len(bids))
	}
	if !bids[0].RemainingSize.Equal(decimal.NewFromInt(6)) {
		t.Fatalf("bid remaining = %s, want 6", bids[0].RemainingSize)
	}
}

func TestOrderBook_Expiration(t *testing.T) {
	ob := NewOrderBook()
	o := model.Order{
		ID:            uuid.New(),
		Pair:          "ETH/USDC",
		Side:          utils.Buy,
		Price:         decimal.NewFromInt(1800),
		Size:          decimal.NewFromInt(5),
		RemainingSize: decimal.NewFromInt(5),
		CommitmentKey: "key1",
		SubmittedAt:   time.Now(),
		ExpiresAt:     time.Now().Add(-1 * time.Second),
	}

	ob.Apply(event.Event{Seq: 1, Type: utils.OrderPlacedType, Data: event.OrderPlaced{Order: o}})

	expired := ob.ExpireOrders(time.Now())
	if len(expired) != 1 {
		t.Fatalf("expired count = %d, want 1", len(expired))
	}

	if got := ob.ActiveOrderCount(); got != 0 {
		t.Fatalf("ActiveOrderCount after expiration = %d, want 0", got)
	}
}

func TestOrderBook_ReplayFromStore(t *testing.T) {
	store := event.NewMemStore()
	o := newOrder(utils.Sell, 2000, 7)

	store.Append(event.Event{Type: utils.OrderPlacedType, Data: event.OrderPlaced{Order: o}})

	ob := NewOrderBook()
	if err := ob.Replay(store); err != nil {
		t.Fatalf("Replay: %v", err)
	}

	if got := ob.ActiveOrderCount(); got != 1 {
		t.Fatalf("ActiveOrderCount after replay = %d, want 1", got)
	}
}
