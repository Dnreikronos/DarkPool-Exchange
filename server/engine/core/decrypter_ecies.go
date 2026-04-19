package core

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/ethereum/go-ethereum/crypto/ecies"
)

// ECIESDecrypter decrypts order ciphertexts produced by ecies.Encrypt against
// the operator's public key (secp256k1). The plaintext is a JSON-encoded
// DecryptedOrder.
//
// Wire layout: client computes
//
//	ct := ecies.Encrypt(pub, json(DecryptedOrder), nil, nil)
//
// and submits ct as the ciphertext bytes. The same secp256k1 key is reused for
// tx signing by EthSubmitter (one operator identity on-chain and off).
type ECIESDecrypter struct {
	priv *ecies.PrivateKey
}

// NewECIESDecrypterFromFile loads a hex-encoded secp256k1 private key (with or
// without 0x prefix, whitespace tolerated) from path.
func NewECIESDecrypterFromFile(path string) (*ECIESDecrypter, error) {
	ecdsaPriv, err := loadOperatorKeyFile(path)
	if err != nil {
		return nil, fmt.Errorf("parse operator privkey: %w", err)
	}
	return &ECIESDecrypter{priv: ecies.ImportECDSA(ecdsaPriv)}, nil
}

func (d *ECIESDecrypter) Decrypt(_ context.Context, ct []byte) (DecryptedOrder, error) {
	pt, err := d.priv.Decrypt(ct, nil, nil)
	if err != nil {
		return DecryptedOrder{}, fmt.Errorf("ecies decrypt: %w", err)
	}
	var out DecryptedOrder
	if err := json.Unmarshal(pt, &out); err != nil {
		return DecryptedOrder{}, fmt.Errorf("unmarshal decrypted order: %w", err)
	}
	return out, nil
}
