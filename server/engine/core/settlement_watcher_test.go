package core

import (
	"context"
	"testing"
	"time"

	coreabi "github.com/darkpool-exchange/server/engine/core/abi"
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

func TestSettlementWatcher_EmitsBatchSettled(t *testing.T) {
	store := event.NewMemStore()
	eng := NewEngine(store, time.Second)

	// Seed a pending batch so we can verify it gets drained.
	batchID := uuid.New()
	auctionID := uuid.New()
	eng.pendingBatches[batchID] = &pendingBatch{BatchID: batchID, AuctionID: auctionID}

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

	w, err := NewSettlementWatcher(client, contractAddr, store, eng)
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

	batchTopic := common.Hash(uuidToBytes32(batchID))
	client.logs <- types.Log{
		Address:     contractAddr,
		Topics:      []common.Hash{topic0, batchTopic},
		BlockNumber: 42,
		TxHash:      common.HexToHash("0xaabbccdd"),
	}

	deadline := time.After(2 * time.Second)
	for {
		if count := countEvents(t, store, utils.BatchSettledType); count == 1 {
			break
		}
		select {
		case <-deadline:
			t.Fatal("BatchSettled event never persisted")
		case <-time.After(20 * time.Millisecond):
		}
	}

	if eng.PendingBatchCount() != 0 {
		t.Errorf("pendingBatches after settle = %d, want 0", eng.PendingBatchCount())
	}

	cancel()
	<-done
}
