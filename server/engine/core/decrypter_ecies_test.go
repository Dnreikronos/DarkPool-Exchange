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

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/crypto/ecies"
	"github.com/shopspring/decimal"
)

func writeKey(t *testing.T) (string, *ecies.PrivateKey) {
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

func TestECIESDecrypter_RoundTrip(t *testing.T) {
	path, priv := writeKey(t)

	dec, err := NewECIESDecrypterFromFile(path)
	if err != nil {
		t.Fatalf("load: %v", err)
	}

	order := DecryptedOrder{
		Pair:          "ETH-USDC",
		Side:          utils.Buy,
		Price:         decimal.NewFromFloat(1800.5),
		Size:          decimal.NewFromFloat(2.25),
		CommitmentKey: "k1",
		TTL:           30 * time.Second,
	}
	pt, _ := json.Marshal(order)

	ct, err := ecies.Encrypt(rand.Reader, &priv.PublicKey, pt, nil, nil)
	if err != nil {
		t.Fatalf("encrypt: %v", err)
	}

	got, err := dec.Decrypt(context.Background(), ct)
	if err != nil {
		t.Fatalf("decrypt: %v", err)
	}
	if got.Pair != order.Pair || !got.Price.Equal(order.Price) || !got.Size.Equal(order.Size) {
		t.Fatalf("roundtrip mismatch: got %+v want %+v", got, order)
	}
}

func TestECIESDecrypter_TamperedCiphertextFails(t *testing.T) {
	path, priv := writeKey(t)
	dec, err := NewECIESDecrypterFromFile(path)
	if err != nil {
		t.Fatal(err)
	}

	pt, _ := json.Marshal(DecryptedOrder{Pair: "X-Y", Size: decimal.NewFromInt(1), Price: decimal.NewFromInt(1)})
	ct, err := ecies.Encrypt(rand.Reader, &priv.PublicKey, pt, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	ct[len(ct)-1] ^= 0xFF

	if _, err := dec.Decrypt(context.Background(), ct); err == nil {
		t.Fatal("want error on tampered ciphertext, got nil")
	}
}

func TestNewECIESDecrypterFromFile_BadKey(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bad")
	if err := os.WriteFile(path, []byte("not-hex"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := NewECIESDecrypterFromFile(path); err == nil {
		t.Fatal("want error on malformed key file")
	}
}
