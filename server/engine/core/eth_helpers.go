package core

import (
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strings"

	"github.com/ethereum/go-ethereum/crypto"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

// loadOperatorKeyFile reads a hex-encoded secp256k1 private key (with or
// without 0x prefix, surrounding whitespace tolerated) and returns the
// parsed ECDSA key. Shared by EthSubmitter and ECIESDecrypter so the on-
// chain signer and the off-chain decrypter always use the same key file.
func loadOperatorKeyFile(path string) (*ecdsa.PrivateKey, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read operator key: %w", err)
	}
	hexStr := strings.TrimSpace(string(raw))
	hexStr = strings.TrimPrefix(hexStr, "0x")
	keyBytes, err := hex.DecodeString(hexStr)
	if err != nil {
		return nil, fmt.Errorf("decode operator key hex: %w", err)
	}
	return crypto.ToECDSA(keyBytes)
}

// uuidToBytes32 left-pads a UUID (16 bytes) into a 32-byte word. The high
// 16 bytes are zero, which the settlement contract treats as unused.
func uuidToBytes32(id uuid.UUID) [32]byte {
	var out [32]byte
	copy(out[16:], id[:])
	return out
}

// decimalToWei scales a shopspring/decimal to a uint256-compatible integer
// using 18 decimals (standard ERC-20 precision). Returns ok=false if the
// value is negative or loses precision.
func decimalToWei(d decimal.Decimal) (*big.Int, bool) {
	if d.Sign() < 0 {
		return nil, false
	}
	scaled := d.Shift(18)
	if !scaled.Equal(scaled.Truncate(0)) {
		return nil, false
	}
	return scaled.BigInt(), true
}
