package settlement

import (
	"context"
	"sync"
	"testing"
	"time"

	coreabi "github.com/darkpool-exchange/server/engine/settlement/abi"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/google/uuid"
)

type fakeSub struct{ err chan error }

func (f *fakeSub) Unsubscribe()      {}
func (f *fakeSub) Err() <-chan error { return f.err }

type fakeLogClient struct {
	logs chan types.Log
	sub  *fakeSub
}

func (c *fakeLogClient) SubscribeFilterLogs(_ context.Context, _ ethereum.FilterQuery, ch chan<- types.Log) (ethereum.Subscription, error) {
	go func() {
		for lg := range c.logs {
			ch <- lg
		}
	}()
	return c.sub, nil
}

type recordingSink struct {
	mu    sync.Mutex
	calls []uuid.UUID
}

func (r *recordingSink) OnBatchSettled(_ event.Event, batchID uuid.UUID) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.calls = append(r.calls, batchID)
}

func (r *recordingSink) count() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.calls)
}

func TestWatcher_EmitsBatchSettled(t *testing.T) {
	store := event.NewMemStore()
	sink := &recordingSink{}

	parsed, err := coreabi.Parsed()
	if err != nil {
		t.Fatal(err)
	}
	topic0 := parsed.Events["BatchSettled"].ID

	client := &fakeLogClient{
		logs: make(chan types.Log, 1),
		sub:  &fakeSub{err: make(chan error, 1)},
	}
	contractAddr := common.HexToAddress("0x0000000000000000000000000000000000001234")

	w, err := NewWatcher(client, contractAddr, store, sink)
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan struct{})
	go func() {
		w.Run(ctx)
		close(done)
	}()

	batchID := uuid.New()
	batchTopic := common.Hash(UUIDToBytes32(batchID))
	client.logs <- types.Log{
		Address:     contractAddr,
		Topics:      []common.Hash{topic0, batchTopic},
		BlockNumber: 42,
		TxHash:      common.HexToHash("0xaabbccdd"),
	}

	deadline := time.After(2 * time.Second)
	for {
		count := 0
		events, _ := store.ReadFrom(0, 64)
		for _, ev := range events {
			if ev.Type == utils.BatchSettledType {
				count++
			}
		}
		if count == 1 {
			break
		}
		select {
		case <-deadline:
			t.Fatal("BatchSettled event never persisted")
		case <-time.After(20 * time.Millisecond):
		}
	}

	if sink.count() != 1 {
		t.Errorf("sink calls = %d, want 1", sink.count())
	}

	cancel()
	<-done
}
