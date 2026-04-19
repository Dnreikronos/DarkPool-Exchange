package core

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	coreabi "github.com/darkpool-exchange/server/engine/core/abi"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/crypto/ecies"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

// onChainStubSubmitter wraps stubSubmitter and emits a synthetic BatchSettled
// log whenever Submit succeeds. Models the production round-trip without a
// real chain: engine submits → stub "lands" tx → watcher observes log.
type onChainStubSubmitter struct {
	stub    *stubSubmitter
	settled chan<- uuid.UUID
}

func (o *onChainStubSubmitter) Submit(ctx context.Context, batchID, auctionID uuid.UUID, matches []event.OrderMatched, proof []byte) (string, error) {
	hash, err := o.stub.Submit(ctx, batchID, auctionID, matches, proof)
	if err != nil {
		return "", err
	}
	o.settled <- batchID
	return hash, nil
}

func writeOperatorKey(t *testing.T) (string, *ecies.PrivateKey) {
	t.Helper()
	priv, err := crypto.GenerateKey()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "op.key")
	if err := os.WriteFile(path, []byte(hex.EncodeToString(crypto.FromECDSA(priv))), 0o600); err != nil {
		t.Fatal(err)
	}
	return path, ecies.ImportECDSA(priv)
}

// TestFullPipeline_EncryptedOrderToSettlement threads every real seam end-to-end:
//   - ECIESDecrypter recovers plaintext from the encrypted order wire payload
//   - stubAggregator produces a deterministic proof
//   - onChainStubSubmitter persists BatchConfirmed and nudges the watcher
//   - SettlementWatcher observes the synthetic log and persists BatchSettled
func TestFullPipeline_EncryptedOrderToSettlement(t *testing.T) {
	keyPath, priv := writeOperatorKey(t)

	store := event.NewMemStore()
	eng := NewEngine(store, time.Second)

	dec, err := NewECIESDecrypterFromFile(keyPath)
	if err != nil {
		t.Fatalf("load decrypter: %v", err)
	}
	eng.SetDecrypter(dec)
	eng.SetAggregator(&stubAggregator{proof: []byte("pipeline-proof")})

	parsed, err := coreabi.Parsed()
	if err != nil {
		t.Fatal(err)
	}
	topic0 := parsed.Events["BatchSettled"].ID
	contract := common.HexToAddress("0x0000000000000000000000000000000000004242")

	logClient := &fakeLogClient{
		logs: make(chan types.Log, 4),
		sub:  &fakeSub{err: make(chan error, 1)},
	}

	settled := make(chan uuid.UUID, 4)
	eng.SetSubmitter(&onChainStubSubmitter{stub: &stubSubmitter{}, settled: settled})

	watcher, err := NewSettlementWatcher(logClient, contract, store, eng)
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go watcher.Run(ctx)
	go func() {
		for batchID := range settled {
			logClient.logs <- types.Log{
				Address:     contract,
				Topics:      []common.Hash{topic0, common.Hash(uuidToBytes32(batchID))},
				BlockNumber: 123,
				TxHash:      common.HexToHash("0xabcdef"),
			}
		}
	}()

	submit := func(side utils.Side, price, size int64, key string) {
		t.Helper()
		plain := DecryptedOrder{
			Pair: "ETH/USDC", Side: side,
			Price: decimal.NewFromInt(price), Size: decimal.NewFromInt(size),
			CommitmentKey: key, TTL: time.Minute,
		}
		pt, _ := json.Marshal(plain)
		ct, err := ecies.Encrypt(rand.Reader, &priv.PublicKey, pt, nil, nil)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := eng.PlaceEncryptedOrder(ctx, ComputeCommitment(plain), nil, ct); err != nil {
			t.Fatalf("place encrypted order: %v", err)
		}
	}
	submit(utils.Buy, 1850, 5, "buyer")
	submit(utils.Sell, 1800, 3, "seller")

	eng.RunAuctionTickCtx(ctx)

	deadline := time.After(3 * time.Second)
	for countEvents(t, store, utils.BatchSettledType) < 1 {
		select {
		case <-deadline:
			t.Fatalf("BatchSettled never persisted (have %d)", countEvents(t, store, utils.BatchSettledType))
		case <-time.After(10 * time.Millisecond):
		}
	}

	assertCount := func(typ utils.EventType, want int) {
		t.Helper()
		if got := countEvents(t, store, typ); got != want {
			t.Errorf("event %v count = %d, want %d", typ, got, want)
		}
	}
	assertCount(utils.OrderPlacedType, 2)
	assertCount(utils.AuctionExecutedType, 1)
	if got := countEvents(t, store, utils.OrderMatchedType); got == 0 {
		t.Error("OrderMatched = 0, want ≥ 1")
	}
	assertCount(utils.BatchSubmittedType, 1)
	assertCount(utils.BatchConfirmedType, 1)
	assertCount(utils.BatchSettledType, 1)

	if got := eng.PendingBatchCount(); got != 0 {
		t.Errorf("pendingBatches after settle = %d, want 0", got)
	}
}
