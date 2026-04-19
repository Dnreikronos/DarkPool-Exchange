package abi

import (
	"fmt"
	"math/big"
	"strings"

	ethabi "github.com/ethereum/go-ethereum/accounts/abi"
)

// MaxMatchesPerBatch mirrors the settlement contract's per-batch cap. The
// engine enforces the same bound in EthSubmitter.Submit so oversize batches
// fail fast off-chain instead of burning gas on a revert.
const MaxMatchesPerBatch = 256

// DarkPoolABI is the minimal surface the Go engine needs: a submitBatch
// write method and a BatchSettled event for the SettlementWatcher.
const DarkPoolABI = `[
  {
    "type": "function",
    "name": "submitBatch",
    "stateMutability": "nonpayable",
    "inputs": [
      {"name": "batchId",   "type": "bytes32"},
      {"name": "auctionId", "type": "bytes32"},
      {"name": "proof",     "type": "bytes"},
      {"name": "matches",   "type": "tuple[]", "components": [
        {"name": "bidOrderId", "type": "bytes32"},
        {"name": "askOrderId", "type": "bytes32"},
        {"name": "price",      "type": "uint256"},
        {"name": "size",       "type": "uint256"}
      ]}
    ],
    "outputs": []
  },
  {
    "type": "event",
    "name": "BatchSettled",
    "anonymous": false,
    "inputs": [
      {"indexed": true, "name": "batchId", "type": "bytes32"}
    ]
  }
]`

// Match mirrors the on-chain tuple. Field order MUST match the ABI above or
// abi.Pack silently produces wrong calldata.
type Match struct {
	BidOrderID [32]byte `abi:"bidOrderId"`
	AskOrderID [32]byte `abi:"askOrderId"`
	Price      *big.Int `abi:"price"`
	Size       *big.Int `abi:"size"`
}

// Parsed returns the parsed abi.ABI. Callers should cache the result.
func Parsed() (ethabi.ABI, error) {
	a, err := ethabi.JSON(strings.NewReader(DarkPoolABI))
	if err != nil {
		return ethabi.ABI{}, fmt.Errorf("parse DarkPool ABI: %w", err)
	}
	return a, nil
}
