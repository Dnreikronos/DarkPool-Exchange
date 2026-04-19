package decrypt

import (
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"os"
	"strings"

	"github.com/ethereum/go-ethereum/crypto"
)

// LoadOperatorKeyFile reads a hex-encoded secp256k1 private key (with or
// without 0x prefix, surrounding whitespace tolerated) and returns the
// parsed ECDSA key. Shared by EthSubmitter and ECIESDecrypter so the on-
// chain signer and the off-chain decrypter always use the same key file.
func LoadOperatorKeyFile(path string) (*ecdsa.PrivateKey, error) {
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
