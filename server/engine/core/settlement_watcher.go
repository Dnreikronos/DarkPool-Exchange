package core

import (
	"context"
	"fmt"
	"log"
	"time"

	coreabi "github.com/darkpool-exchange/server/engine/core/abi"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/google/uuid"
)

// LogSubscriber is the subset of ethclient the SettlementWatcher needs.
// Extracted so tests can drive a fake log channel without an RPC endpoint.
type LogSubscriber interface {
	SubscribeFilterLogs(ctx context.Context, q ethereum.FilterQuery, ch chan<- types.Log) (ethereum.Subscription, error)
}

// SettlementWatcher subscribes to BatchSettled logs on the DarkPool contract,
// persists a BatchSettled event per receipt, and nudges the engine to drop
// the matching pendingBatch. Reconnects with a bounded backoff on subscription
// drops so a brief RPC outage doesn't wedge settlement finality forever.
type SettlementWatcher struct {
	client   LogSubscriber
	store    event.Store
	engine   *Engine
	contract common.Address
	topic0   common.Hash
}

func NewSettlementWatcher(client LogSubscriber, contractAddr common.Address, store event.Store, engine *Engine) (*SettlementWatcher, error) {
	parsed, err := coreabi.Parsed()
	if err != nil {
		return nil, err
	}
	ev, ok := parsed.Events["BatchSettled"]
	if !ok {
		return nil, fmt.Errorf("BatchSettled missing from DarkPool ABI")
	}
	return &SettlementWatcher{
		client:   client,
		store:    store,
		engine:   engine,
		contract: contractAddr,
		topic0:   ev.ID,
	}, nil
}

// Run blocks until ctx is done. Intended to be launched in a goroutine by
// cmd/server/main.go.
func (w *SettlementWatcher) Run(ctx context.Context) {
	const minBackoff = time.Second
	const maxBackoff = 30 * time.Second
	backoff := minBackoff

	for {
		receivedAny, err := w.runOnce(ctx)
		if err != nil {
			if ctx.Err() != nil {
				return
			}
			// Any successful log delivery resets backoff: a long-stable
			// subscription that drops after hours shouldn't inherit a stale
			// maxBackoff from a prior reconnect storm.
			if receivedAny {
				backoff = minBackoff
			}
			log.Printf("settlement watcher: %v (reconnect in %s)", err, backoff)
			select {
			case <-ctx.Done():
				return
			case <-time.After(backoff):
			}
			backoff *= 2
			if backoff > maxBackoff {
				backoff = maxBackoff
			}
			continue
		}
		return
	}
}

func (w *SettlementWatcher) runOnce(ctx context.Context) (receivedAny bool, err error) {
	ch := make(chan types.Log, 16)
	sub, subErr := w.client.SubscribeFilterLogs(ctx, ethereum.FilterQuery{
		Addresses: []common.Address{w.contract},
		Topics:    [][]common.Hash{{w.topic0}},
	}, ch)
	if subErr != nil {
		return false, fmt.Errorf("subscribe BatchSettled: %w", subErr)
	}
	defer sub.Unsubscribe()

	for {
		select {
		case <-ctx.Done():
			return receivedAny, nil
		case subErr := <-sub.Err():
			if subErr == nil {
				return receivedAny, fmt.Errorf("subscription closed")
			}
			return receivedAny, fmt.Errorf("subscription error: %w", subErr)
		case vLog := <-ch:
			receivedAny = true
			if err := w.handleLog(vLog); err != nil {
				log.Printf("settlement watcher: handle log: %v", err)
			}
		}
	}
}

func (w *SettlementWatcher) handleLog(vLog types.Log) error {
	if len(vLog.Topics) < 2 {
		return fmt.Errorf("BatchSettled log missing indexed batchId")
	}
	batchIDBytes := vLog.Topics[1]
	batchID, err := bytes32ToUUID(batchIDBytes)
	if err != nil {
		return fmt.Errorf("decode batchId: %w", err)
	}

	// blockNumber and txHash are read from the log envelope, not the event
	// data — the node populates both authoritatively, so the contract
	// doesn't need to re-emit them as non-indexed fields.
	blockNumber := vLog.BlockNumber
	txHash := vLog.TxHash.Hex()

	evt := event.Event{
		Type:      utils.BatchSettledType,
		Timestamp: time.Now(),
		Data: event.BatchSettled{
			BatchID:     batchID,
			BlockNumber: blockNumber,
			TxHash:      txHash,
		},
	}
	if err := w.store.Append(&evt); err != nil {
		return fmt.Errorf("persist BatchSettled: %w", err)
	}
	w.engine.onBatchSettled(evt, batchID)
	return nil
}

// bytes32ToUUID reverses uuidToBytes32 — the UUID sits in the low 16 bytes.
// Returns error if the high 16 bytes are non-zero, which would indicate the
// batchId came from a different producer than this engine.
func bytes32ToUUID(b [32]byte) (uuid.UUID, error) {
	for i := 0; i < 16; i++ {
		if b[i] != 0 {
			return uuid.UUID{}, fmt.Errorf("non-UUID batchId %s", common.Hash(b).Hex())
		}
	}
	var id uuid.UUID
	copy(id[:], b[16:])
	return id, nil
}

