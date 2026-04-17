package core

import (
	"context"
	"errors"
	"path/filepath"
	"testing"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

// stubSubmitter is a single configurable Submitter test double. fail=true
// makes Submit return an error (simulating chain unavailability or timeout);
// fail=false returns a synthetic tx hash. calls counts total invocations.
type stubSubmitter struct {
	fail  bool
	calls int
}

func (s *stubSubmitter) Submit(_ context.Context, batchID uuid.UUID, _ uuid.UUID, _ []event.OrderMatched) (string, error) {
	s.calls++
	if s.fail {
		return "", errors.New("submit boom")
	}
	return "chain:" + batchID.String(), nil
}

func countEvents(t *testing.T, store event.Store, target utils.EventType) int {
	t.Helper()
	n := 0
	var after uint64
	for {
		evts, err := store.ReadFrom(after, 256)
		if err != nil {
			t.Fatalf("ReadFrom: %v", err)
		}
		if len(evts) == 0 {
			return n
		}
		for _, ev := range evts {
			if ev.Type == target {
				n++
			}
		}
		after = evts[len(evts)-1].Seq
	}
}

func TestBatchLifecycle_NoopSubmitter(t *testing.T) {
	store := event.NewMemStore()
	e := NewEngine(store, time.Second)

	e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0)
	e.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0)

	notifications := e.RunAuctionTickCtx(context.Background())
	if len(notifications) != 1 {
		t.Fatalf("notifications = %d, want 1", len(notifications))
	}

	if got := e.PendingBatchCount(); got != 0 {
		t.Errorf("pending = %d, want 0 after successful submit", got)
	}

	if got := countEvents(t, store, utils.BatchSubmittedType); got != 1 {
		t.Errorf("BatchSubmitted events = %d, want 1", got)
	}
	if got := countEvents(t, store, utils.BatchConfirmedType); got != 1 {
		t.Errorf("BatchConfirmed events = %d, want 1", got)
	}
}

func TestBatchLifecycle_CrashBetweenSubmitAndConfirm(t *testing.T) {
	store := event.NewMemStore()

	e1 := NewEngine(store, time.Second)
	fail := &stubSubmitter{fail: true}
	e1.SetSubmitter(fail)

	e1.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0)
	e1.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0)

	e1.RunAuctionTickCtx(context.Background())

	if fail.calls != 1 {
		t.Fatalf("submitter calls = %d, want 1", fail.calls)
	}
	if got := e1.PendingBatchCount(); got != 1 {
		t.Fatalf("pending after failed submit = %d, want 1", got)
	}
	if got := countEvents(t, store, utils.BatchSubmittedType); got != 1 {
		t.Errorf("BatchSubmitted = %d, want 1", got)
	}
	if got := countEvents(t, store, utils.BatchConfirmedType); got != 0 {
		t.Errorf("BatchConfirmed = %d, want 0 (crashed before confirm)", got)
	}

	// simulate restart: new engine, same store
	e2 := NewEngine(store, time.Second)
	ok := &stubSubmitter{}
	e2.SetSubmitter(ok)
	if err := e2.Recover(); err != nil {
		t.Fatalf("Recover: %v", err)
	}
	if got := e2.PendingBatchCount(); got != 1 {
		t.Fatalf("pending after recover = %d, want 1", got)
	}

	e2.ResubmitPending(context.Background())

	if ok.calls != 1 {
		t.Errorf("resubmit calls = %d, want 1", ok.calls)
	}
	if got := e2.PendingBatchCount(); got != 0 {
		t.Errorf("pending after resubmit = %d, want 0", got)
	}
	if got := countEvents(t, store, utils.BatchConfirmedType); got != 1 {
		t.Errorf("BatchConfirmed after resubmit = %d, want 1", got)
	}
}

func TestBatchLifecycle_RecoverFromFileStore(t *testing.T) {
	path := filepath.Join(t.TempDir(), "events.log")

	store1, err := event.OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}

	e1 := NewEngine(store1, time.Second)
	fail := &stubSubmitter{fail: true}
	e1.SetSubmitter(fail)

	e1.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0)
	e1.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0)
	e1.RunAuctionTickCtx(context.Background())

	if got := e1.PendingBatchCount(); got != 1 {
		t.Fatalf("e1 pending = %d, want 1", got)
	}
	if err := store1.Close(); err != nil {
		t.Fatalf("Close store1: %v", err)
	}

	// simulate restart
	store2, err := event.OpenFileStore(path)
	if err != nil {
		t.Fatalf("Reopen: %v", err)
	}
	t.Cleanup(func() { store2.Close() })

	e2 := NewEngine(store2, time.Second)
	ok := &stubSubmitter{}
	e2.SetSubmitter(ok)
	if err := e2.Recover(); err != nil {
		t.Fatalf("Recover: %v", err)
	}
	if got := e2.PendingBatchCount(); got != 1 {
		t.Fatalf("pending after recover = %d, want 1", got)
	}

	e2.ResubmitPending(context.Background())

	if got := e2.PendingBatchCount(); got != 0 {
		t.Errorf("pending after resubmit = %d, want 0", got)
	}
	if got := countEvents(t, store2, utils.BatchConfirmedType); got != 1 {
		t.Errorf("BatchConfirmed = %d, want 1", got)
	}
	if ok.calls != 1 {
		t.Errorf("stubSubmitter calls = %d, want 1", ok.calls)
	}
}

func TestBatchLifecycle_RetriesOnNextTick(t *testing.T) {
	store := event.NewMemStore()
	e := NewEngine(store, time.Second)
	sub := &stubSubmitter{fail: true}
	e.SetSubmitter(sub)
	e.SetRetryBackoff(0, 0)

	// Tick 1: auction matches, submit fails, batch stays pending.
	e.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0)
	e.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0)
	e.RunAuctionTickCtx(context.Background())

	if got := e.PendingBatchCount(); got != 1 {
		t.Fatalf("pending after failed tick = %d, want 1", got)
	}
	tick1Calls := sub.calls

	// Submitter recovers. No new orders placed, so no new auction fires,
	// but the pending batch from tick 1 should be retried on tick 2.
	sub.fail = false
	e.RunAuctionTickCtx(context.Background())

	if sub.calls != tick1Calls+1 {
		t.Errorf("tick2 calls = %d, want %d (one retry)", sub.calls, tick1Calls+1)
	}
	if got := e.PendingBatchCount(); got != 0 {
		t.Errorf("pending after retry tick = %d, want 0", got)
	}
	if got := countEvents(t, store, utils.BatchConfirmedType); got != 1 {
		t.Errorf("BatchConfirmed = %d, want 1", got)
	}
}

func TestBatchLifecycle_StartReplaysPending(t *testing.T) {
	store := event.NewMemStore()

	e1 := NewEngine(store, time.Second)
	e1.SetSubmitter(&stubSubmitter{fail: true})

	e1.PlaceOrder("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0)
	e1.PlaceOrder("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0)
	e1.RunAuctionTickCtx(context.Background())

	e2 := NewEngine(store, 10*time.Millisecond)
	ok := &stubSubmitter{}
	e2.SetSubmitter(ok)
	if err := e2.Recover(); err != nil {
		t.Fatalf("Recover: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	done := make(chan struct{})
	go func() {
		e2.Start(ctx)
		close(done)
	}()
	<-done

	if got := e2.PendingBatchCount(); got != 0 {
		t.Errorf("pending after Start = %d, want 0", got)
	}
	if ok.calls != 1 {
		t.Errorf("submitter calls = %d, want 1", ok.calls)
	}
}
