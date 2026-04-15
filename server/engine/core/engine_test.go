package core

import (
	"context"
	"testing"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/shopspring/decimal"
)

func TestPlaceOrder(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	order, err := e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1800), decimal.NewFromInt(10), "key-1", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if order.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", order.Pair)
	}
	if order.Side != utils.Buy {
		t.Errorf("side = %v, want Buy", order.Side)
	}
	if e.ActiveOrderCount() != 1 {
		t.Errorf("active count = %d, want 1", e.ActiveOrderCount())
	}
}

func TestPlaceOrderValidation(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	tests := []struct {
		name          string
		pair          string
		price         decimal.Decimal
		size          decimal.Decimal
		commitmentKey string
	}{
		{"empty pair", "", decimal.NewFromInt(100), decimal.NewFromInt(1), "key"},
		{"zero price", "ETH/USDC", decimal.Zero, decimal.NewFromInt(1), "key"},
		{"negative size", "ETH/USDC", decimal.NewFromInt(100), decimal.NewFromInt(-1), "key"},
		{"empty commitment", "ETH/USDC", decimal.NewFromInt(100), decimal.NewFromInt(1), ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := e.PlaceOrder(tt.pair, utils.Buy, tt.price, tt.size, tt.commitmentKey, 0)
			if err == nil {
				t.Error("expected error, got nil")
			}
		})
	}
}

func TestCancelOrder(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	order, _ := e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1800), decimal.NewFromInt(10), "key-1", 0)

	if err := e.CancelOrder(order.ID, ""); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if e.ActiveOrderCount() != 0 {
		t.Errorf("active count = %d, want 0", e.ActiveOrderCount())
	}

	if got := e.GetOrder(order.ID); got != nil {
		t.Error("cancelled order should not be found")
	}
}

func TestRunAuctionTick(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	notifications := e.RunAuctionTick()
	if len(notifications) != 1 {
		t.Fatalf("notifications = %d, want 1", len(notifications))
	}

	n := notifications[0]
	if n.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", n.Pair)
	}
	if n.MatchCount != 1 {
		t.Errorf("match count = %d, want 1", n.MatchCount)
	}
	if !n.MatchedVolume.Equal(decimal.NewFromInt(3)) {
		t.Errorf("matched volume = %s, want 3", n.MatchedVolume)
	}
}

func TestSubscribeReceivesNotifications(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	sub := e.Subscribe(4)
	defer e.Unsubscribe(sub.ID)

	e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	e.RunAuctionTick()

	select {
	case n := <-sub.Ch:
		if n.Pair != "ETH/USDC" {
			t.Errorf("pair = %s, want ETH/USDC", n.Pair)
		}
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for notification")
	}
}

func TestStartStopsOnContextCancel(t *testing.T) {
	e := NewEngine(event.NewMemStore(), 50*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()

	done := make(chan struct{})
	go func() {
		e.Start(ctx)
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("Start did not return after context cancellation")
	}
}

func TestGetAuctionHistory(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)
	e.RunAuctionTick()

	history, err := e.GetAuctionHistory("ETH/USDC", 10)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(history) != 1 {
		t.Fatalf("history len = %d, want 1", len(history))
	}
	if history[0].Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", history[0].Pair)
	}
}

func TestRecoverFromEventStore(t *testing.T) {
	store := event.NewMemStore()
	e1 := NewEngine(store, time.Second)

	e1.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e1.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	// Simulate crash: new engine, same store
	e2 := NewEngine(store, time.Second)
	if err := e2.Recover(); err != nil {
		t.Fatalf("recover error: %v", err)
	}
	if e2.ActiveOrderCount() != 2 {
		t.Errorf("active count after recovery = %d, want 2", e2.ActiveOrderCount())
	}
}
