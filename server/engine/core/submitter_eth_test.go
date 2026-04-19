package core

import (
	"context"
	"math/big"
	"testing"

	coreabi "github.com/darkpool-exchange/server/engine/core/abi"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/model"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

// fakeEthClient implements the EthClient surface without a real RPC or chain.
// Captures SendTransaction calls and returns stubbed values for the read paths.
type fakeEthClient struct {
	nonce    uint64
	gasPrice *big.Int
	sent     []*types.Transaction
	sendErr  error
}

func (f *fakeEthClient) ChainID(_ context.Context) (*big.Int, error) {
	return big.NewInt(1337), nil
}
func (f *fakeEthClient) PendingNonceAt(_ context.Context, _ common.Address) (uint64, error) {
	return f.nonce, nil
}
func (f *fakeEthClient) SuggestGasPrice(_ context.Context) (*big.Int, error) {
	if f.gasPrice == nil {
		return big.NewInt(1_000_000_000), nil
	}
	return f.gasPrice, nil
}
func (f *fakeEthClient) SuggestGasTipCap(_ context.Context) (*big.Int, error) {
	return big.NewInt(1_000_000_000), nil
}
func (f *fakeEthClient) EstimateGas(_ context.Context, _ ethereum.CallMsg) (uint64, error) {
	return 21_000, nil
}
func (f *fakeEthClient) SendTransaction(_ context.Context, tx *types.Transaction) error {
	if f.sendErr != nil {
		return f.sendErr
	}
	f.sent = append(f.sent, tx)
	return nil
}
func (f *fakeEthClient) CallContract(_ context.Context, _ ethereum.CallMsg, _ *big.Int) ([]byte, error) {
	return nil, nil
}
func (f *fakeEthClient) CodeAt(_ context.Context, _ common.Address, _ *big.Int) ([]byte, error) {
	return []byte{1}, nil
}
func (f *fakeEthClient) HeaderByNumber(_ context.Context, _ *big.Int) (*types.Header, error) {
	return &types.Header{Number: big.NewInt(1), BaseFee: big.NewInt(1)}, nil
}
func (f *fakeEthClient) PendingCodeAt(_ context.Context, _ common.Address) ([]byte, error) {
	return []byte{1}, nil
}
func (f *fakeEthClient) FilterLogs(_ context.Context, _ ethereum.FilterQuery) ([]types.Log, error) {
	return nil, nil
}
func (f *fakeEthClient) SubscribeFilterLogs(_ context.Context, _ ethereum.FilterQuery, _ chan<- types.Log) (ethereum.Subscription, error) {
	return nil, nil
}

func newTestSubmitter(t *testing.T, client EthClient) *EthSubmitter {
	t.Helper()
	priv, err := crypto.GenerateKey()
	if err != nil {
		t.Fatal(err)
	}
	s, err := newEthSubmitter(client, EthSubmitterConfig{
		RPCURL:          "unused",
		ContractAddress: "0x0000000000000000000000000000000000001234",
		ChainID:         1337,
		GasLimit:        500_000,
	}, priv)
	if err != nil {
		t.Fatalf("newEthSubmitter: %v", err)
	}
	return s
}

func sampleEthMatch() event.OrderMatched {
	return event.OrderMatched{
		AuctionID: uuid.New(),
		Bid:       model.Fill{OrderID: uuid.New(), Size: decimal.NewFromInt(2)},
		Ask:       model.Fill{OrderID: uuid.New(), Size: decimal.NewFromInt(2)},
		Price:     decimal.NewFromInt(1800),
		Size:      decimal.NewFromInt(2),
	}
}

func TestEthSubmitter_PackSubmitRoundTrip(t *testing.T) {
	client := &fakeEthClient{}
	s := newTestSubmitter(t, client)

	batchID := uuid.New()
	auctionID := uuid.New()
	matches := []event.OrderMatched{sampleEthMatch()}
	proof := []byte{0xDE, 0xAD, 0xBE, 0xEF}

	data, err := s.PackSubmit(batchID, auctionID, matches, proof)
	if err != nil {
		t.Fatalf("pack: %v", err)
	}
	if len(data) < 4 {
		t.Fatal("calldata too short")
	}

	method, err := s.abi.MethodById(data[:4])
	if err != nil {
		t.Fatalf("decode selector: %v", err)
	}
	if method.Name != "submitBatch" {
		t.Errorf("selector = %s, want submitBatch", method.Name)
	}
}

func TestEthSubmitter_Enforces256Cap(t *testing.T) {
	client := &fakeEthClient{}
	s := newTestSubmitter(t, client)

	big := make([]event.OrderMatched, coreabi.MaxMatchesPerBatch+1)
	for i := range big {
		big[i] = sampleEthMatch()
	}
	if _, err := s.PackSubmit(uuid.New(), uuid.New(), big, nil); err == nil {
		t.Fatal("want error when exceeding MaxMatchesPerBatch")
	}
}

func TestEthSubmitter_Submit_SignsAndSends(t *testing.T) {
	client := &fakeEthClient{nonce: 7}
	s := newTestSubmitter(t, client)

	txHash, err := s.Submit(context.Background(), uuid.New(), uuid.New(), []event.OrderMatched{sampleEthMatch()}, []byte{1, 2, 3})
	if err != nil {
		t.Fatalf("submit: %v", err)
	}
	if txHash == "" {
		t.Fatal("empty tx hash")
	}
	if len(client.sent) != 1 {
		t.Fatalf("sent %d txs, want 1", len(client.sent))
	}
	if got := client.sent[0].Nonce(); got != 7 {
		t.Errorf("nonce = %d, want 7", got)
	}
	if got := client.sent[0].To(); got == nil || got.Hex() != "0x0000000000000000000000000000000000001234" {
		t.Errorf("to addr = %v, want DarkPool address", got)
	}
}
