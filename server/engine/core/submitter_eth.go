package core

import (
	"context"
	"crypto/ecdsa"
	"fmt"
	"math/big"

	coreabi "github.com/darkpool-exchange/server/engine/core/abi"
	"github.com/darkpool-exchange/server/engine/event"

	ethabi "github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/google/uuid"
)

// EthClient is the subset of ethclient.Client EthSubmitter needs. Declared as
// an interface so tests can inject fakes without spinning up a simulated
// backend for every submit path.
type EthClient interface {
	bind.ContractBackend
	ChainID(ctx context.Context) (*big.Int, error)
}

// EthSubmitter packs matched pairs + aggregated proof into a submitBatch
// calldata, signs the tx with the operator key, and sends it via ethclient.
// Idempotency guarantee (per Submitter interface): repeated calls with the
// same batchID rely on the on-chain require(!batches[id].settled) check; the
// second tx reverts and this Submitter surfaces that as an error, matching
// the existing retry semantics in Engine.submitBatch.
type EthSubmitter struct {
	client          EthClient
	abi             ethabi.ABI
	contractAddress common.Address
	priv            *ecdsa.PrivateKey
	chainID         *big.Int
	gasLimit        uint64
}

// EthSubmitterConfig mirrors the relevant fields from config.Config to keep
// this package decoupled from api/config.
type EthSubmitterConfig struct {
	RPCURL          string
	OperatorKeyPath string
	ContractAddress string
	ChainID         uint64
	GasLimit        uint64
}

// NewEthSubmitter dials the RPC, loads the operator key, and prepares the
// packed ABI for repeated submits.
func NewEthSubmitter(ctx context.Context, cfg EthSubmitterConfig) (*EthSubmitter, error) {
	if cfg.RPCURL == "" {
		return nil, fmt.Errorf("eth RPC URL required")
	}
	if !common.IsHexAddress(cfg.ContractAddress) {
		return nil, fmt.Errorf("invalid DarkPool address %q", cfg.ContractAddress)
	}
	priv, err := loadOperatorKeyFile(cfg.OperatorKeyPath)
	if err != nil {
		return nil, fmt.Errorf("load operator key: %w", err)
	}
	client, err := ethclient.DialContext(ctx, cfg.RPCURL)
	if err != nil {
		return nil, fmt.Errorf("dial eth rpc: %w", err)
	}
	return newEthSubmitter(client, cfg, priv)
}

func newEthSubmitter(client EthClient, cfg EthSubmitterConfig, priv *ecdsa.PrivateKey) (*EthSubmitter, error) {
	parsed, err := coreabi.Parsed()
	if err != nil {
		return nil, err
	}
	chainID := big.NewInt(0).SetUint64(cfg.ChainID)
	if cfg.ChainID == 0 {
		// Fall back to whatever the RPC reports.
		id, err := client.ChainID(context.Background())
		if err != nil {
			return nil, fmt.Errorf("query chain id: %w", err)
		}
		chainID = id
	}
	gas := cfg.GasLimit
	if gas == 0 {
		gas = 500_000
	}
	return &EthSubmitter{
		client:          client,
		abi:             parsed,
		contractAddress: common.HexToAddress(cfg.ContractAddress),
		priv:            priv,
		chainID:         chainID,
		gasLimit:        gas,
	}, nil
}

// Client exposes the underlying backend so the engine can plumb it into the
// SettlementWatcher without re-dialing.
func (s *EthSubmitter) Client() EthClient { return s.client }

// Address returns the configured DarkPool contract address.
func (s *EthSubmitter) Address() common.Address { return s.contractAddress }

// PackSubmit is exposed for tests: deterministic calldata for a given batch.
func (s *EthSubmitter) PackSubmit(batchID, auctionID uuid.UUID, matches []event.OrderMatched, proof []byte) ([]byte, error) {
	if len(matches) > coreabi.MaxMatchesPerBatch {
		return nil, fmt.Errorf("batch has %d matches, exceeds on-chain cap of %d", len(matches), coreabi.MaxMatchesPerBatch)
	}
	packed := make([]coreabi.Match, 0, len(matches))
	for _, m := range matches {
		price, ok := decimalToWei(m.Price)
		if !ok {
			return nil, fmt.Errorf("price %s cannot be represented on-chain", m.Price)
		}
		size, ok := decimalToWei(m.Size)
		if !ok {
			return nil, fmt.Errorf("size %s cannot be represented on-chain", m.Size)
		}
		packed = append(packed, coreabi.Match{
			BidOrderID: uuidToBytes32(m.Bid.OrderID),
			AskOrderID: uuidToBytes32(m.Ask.OrderID),
			Price:      price,
			Size:       size,
		})
	}
	return s.abi.Pack("submitBatch",
		uuidToBytes32(batchID),
		uuidToBytes32(auctionID),
		proof,
		packed,
	)
}

// Submit signs + sends the tx, returning the tx hash. Does NOT wait for
// mining; Engine.submitBatch treats the returned hash as "RPC accepted" and
// SettlementWatcher handles finality via the BatchSettled log.
func (s *EthSubmitter) Submit(ctx context.Context, batchID, auctionID uuid.UUID, matches []event.OrderMatched, proof []byte) (string, error) {
	calldata, err := s.PackSubmit(batchID, auctionID, matches, proof)
	if err != nil {
		return "", err
	}

	auth, err := bind.NewKeyedTransactorWithChainID(s.priv, s.chainID)
	if err != nil {
		return "", fmt.Errorf("build transactor: %w", err)
	}
	auth.Context = ctx
	auth.GasLimit = s.gasLimit

	nonce, err := s.client.PendingNonceAt(ctx, auth.From)
	if err != nil {
		return "", fmt.Errorf("pending nonce: %w", err)
	}
	tip, err := s.client.SuggestGasTipCap(ctx)
	if err != nil {
		return "", fmt.Errorf("suggest gas tip cap: %w", err)
	}
	head, err := s.client.HeaderByNumber(ctx, nil)
	if err != nil {
		return "", fmt.Errorf("header by number: %w", err)
	}
	if head.BaseFee == nil {
		return "", fmt.Errorf("chain is pre-EIP-1559 (nil BaseFee)")
	}
	// Standard pattern: cap = 2*baseFee + tip. Covers one base-fee doubling
	// before the tx gets mined; excess is refunded by the protocol.
	gasFeeCap := new(big.Int).Add(new(big.Int).Mul(head.BaseFee, big.NewInt(2)), tip)

	to := s.contractAddress
	rawTx := types.NewTx(&types.DynamicFeeTx{
		ChainID:   s.chainID,
		Nonce:     nonce,
		GasTipCap: tip,
		GasFeeCap: gasFeeCap,
		Gas:       s.gasLimit,
		To:        &to,
		Value:     big.NewInt(0),
		Data:      calldata,
	})
	signed, err := auth.Signer(auth.From, rawTx)
	if err != nil {
		return "", fmt.Errorf("sign tx: %w", err)
	}
	if err := s.client.SendTransaction(ctx, signed); err != nil {
		return "", fmt.Errorf("send tx: %w", err)
	}
	return signed.Hash().Hex(), nil
}
