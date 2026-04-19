package core

import (
	"bytes"
	"context"
	"encoding/gob"
	"encoding/json"
	"errors"
	"path/filepath"
	"testing"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

func TestPlaceOrder(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	order, err := e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1800), decimal.NewFromInt(10), "key-1", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if order.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", order.Pair)
	}
	if order.Side != utils.Buy {
		t.Errorf("side = %v, want Buy", order.Side)
	}
	if e.ActiveOrderCount() != 1 {
		t.Errorf("active count = %d, want 1", e.ActiveOrderCount())
	}
}

func TestPlaceOrderValidation(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	tests := []struct {
		name          string
		pair          string
		price         decimal.Decimal
		size          decimal.Decimal
		commitmentKey string
	}{
		{"empty pair", "", decimal.NewFromInt(100), decimal.NewFromInt(1), "key"},
		{"zero price", "ETH/USDC", decimal.Zero, decimal.NewFromInt(1), "key"},
		{"negative size", "ETH/USDC", decimal.NewFromInt(100), decimal.NewFromInt(-1), "key"},
		{"empty commitment", "ETH/USDC", decimal.NewFromInt(100), decimal.NewFromInt(1), ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := e.placeOrderPlaintext(tt.pair, utils.Buy, tt.price, tt.size, tt.commitmentKey, 0)
			if err == nil {
				t.Error("expected error, got nil")
			}
		})
	}
}

func TestCancelOrder(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	order, _ := e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1800), decimal.NewFromInt(10), "key-1", 0)

	if err := e.CancelOrder(order.ID, ""); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if e.ActiveOrderCount() != 0 {
		t.Errorf("active count = %d, want 0", e.ActiveOrderCount())
	}

	if got := e.GetOrder(order.ID); got != nil {
		t.Error("cancelled order should not be found")
	}
}

func TestRunAuctionTick(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	notifications := e.RunAuctionTickCtx(context.Background())
	if len(notifications) != 1 {
		t.Fatalf("notifications = %d, want 1", len(notifications))
	}

	n := notifications[0]
	if n.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", n.Pair)
	}
	if n.MatchCount != 1 {
		t.Errorf("match count = %d, want 1", n.MatchCount)
	}
	if !n.MatchedVolume.Equal(decimal.NewFromInt(3)) {
		t.Errorf("matched volume = %s, want 3", n.MatchedVolume)
	}
}

func TestSubscribeReceivesNotifications(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	sub := e.Subscribe(4)
	defer e.Unsubscribe(sub.ID)

	e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	e.RunAuctionTickCtx(context.Background())

	select {
	case n := <-sub.Ch:
		if n.Pair != "ETH/USDC" {
			t.Errorf("pair = %s, want ETH/USDC", n.Pair)
		}
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for notification")
	}
}

func TestStartStopsOnContextCancel(t *testing.T) {
	e := NewEngine(event.NewMemStore(), 50*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()

	done := make(chan struct{})
	go func() {
		e.Start(ctx)
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("Start did not return after context cancellation")
	}
}

func TestGetAuctionHistory(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)
	e.RunAuctionTickCtx(context.Background())

	history, err := e.GetAuctionHistory("ETH/USDC", 10)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(history) != 1 {
		t.Fatalf("history len = %d, want 1", len(history))
	}
	if history[0].Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", history[0].Pair)
	}
}

func encodeDecrypted(t *testing.T, d DecryptedOrder) []byte {
	t.Helper()
	b, err := json.Marshal(d)
	if err != nil {
		t.Fatalf("marshal DecryptedOrder: %v", err)
	}
	return b
}

func TestPlaceEncryptedOrder_NoopRoundTrip(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	d := DecryptedOrder{
		Pair:          "ETH/USDC",
		Side:          utils.Buy,
		Price:         decimal.NewFromInt(1800),
		Size:          decimal.NewFromInt(10),
		CommitmentKey: "ck-1",
		TTL:           60 * time.Second,
	}
	ct := encodeDecrypted(t, d)
	commitment := ComputeCommitment(d)

	order, err := e.PlaceEncryptedOrder(context.Background(), commitment, []byte("stub-proof"), ct)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if order.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", order.Pair)
	}
	if order.Side != utils.Buy {
		t.Errorf("side = %v, want Buy", order.Side)
	}
	if e.ActiveOrderCount() != 1 {
		t.Errorf("active count = %d, want 1", e.ActiveOrderCount())
	}
}

func TestPlaceEncryptedOrder_CommitmentMismatch(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	d := DecryptedOrder{
		Pair:          "ETH/USDC",
		Side:          utils.Buy,
		Price:         decimal.NewFromInt(1800),
		Size:          decimal.NewFromInt(10),
		CommitmentKey: "ck-1",
		TTL:           60 * time.Second,
	}
	ct := encodeDecrypted(t, d)

	other := d
	other.Price = decimal.NewFromInt(9999)
	wrongCommitment := ComputeCommitment(other)

	_, err := e.PlaceEncryptedOrder(context.Background(), wrongCommitment, []byte("stub-proof"), ct)
	if !errors.Is(err, utils.ErrCommitmentMismatch) {
		t.Fatalf("err = %v, want ErrCommitmentMismatch", err)
	}
	if e.ActiveOrderCount() != 0 {
		t.Errorf("active count = %d, want 0 (order must not land)", e.ActiveOrderCount())
	}
}

// xorDecrypter is a test-only obfuscating decrypter. Real crypto is deferred
// to later phases; here we need *any* scheme whose ciphertext bytes don't
// contain the plaintext so the privacy canary is meaningful.
type xorDecrypter struct{ key byte }

func (x xorDecrypter) Encrypt(plaintext []byte) []byte {
	out := make([]byte, len(plaintext))
	for i, b := range plaintext {
		out[i] = b ^ x.key
	}
	return out
}

func (x xorDecrypter) Decrypt(_ context.Context, ct []byte) (DecryptedOrder, error) {
	return NoopDecrypter{}.Decrypt(context.Background(), x.Encrypt(ct))
}

// TestEventStoreContainsNoPlaintext is the dark-pool privacy invariant: a
// distinctive plaintext price must never appear in any persisted event's
// serialized bytes. If it does, the event log is leaking order data.
func TestEventStoreContainsNoPlaintext(t *testing.T) {
	store := event.NewMemStore()
	e := NewEngine(store, time.Second)
	dec := xorDecrypter{key: 0x5A}
	e.SetDecrypter(dec)

	d := DecryptedOrder{
		Pair:          "ETH/USDC",
		Side:          utils.Buy,
		Price:         decimal.RequireFromString("1234.5678"),
		Size:          decimal.RequireFromString("42.1337"),
		CommitmentKey: "secret-key-abc",
		TTL:           60 * time.Second,
	}
	ct := dec.Encrypt(encodeDecrypted(t, d))
	commitment := ComputeCommitment(d)

	if _, err := e.PlaceEncryptedOrder(context.Background(), commitment, []byte("stub"), ct); err != nil {
		t.Fatalf("place: %v", err)
	}

	needles := [][]byte{
		[]byte("1234.5678"),
		[]byte("42.1337"),
		[]byte("secret-key-abc"),
	}

	var after uint64
	for {
		evts, err := store.ReadFrom(after, 256)
		if err != nil {
			t.Fatalf("ReadFrom: %v", err)
		}
		if len(evts) == 0 {
			break
		}
		for _, ev := range evts {
			var buf bytes.Buffer
			if err := gob.NewEncoder(&buf).Encode(ev); err != nil {
				t.Fatalf("encode: %v", err)
			}
			for _, needle := range needles {
				if bytes.Contains(buf.Bytes(), needle) {
					t.Errorf("plaintext %q leaked into event seq=%d type=%v", needle, ev.Seq, ev.Type)
				}
			}
		}
		after = evts[len(evts)-1].Seq
	}
}

func TestPlaceEncryptedOrder_BadCiphertext(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)

	_, err := e.PlaceEncryptedOrder(context.Background(), []byte("x"), []byte("stub-proof"), []byte("not-json"))
	if err == nil {
		t.Fatal("expected error, got nil")
	}
}

func TestRecoverFromEventStore(t *testing.T) {
	store := event.NewMemStore()
	e1 := NewEngine(store, time.Second)

	e1.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e1.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)

	// Simulate crash: new engine, same store
	e2 := NewEngine(store, time.Second)
	if err := e2.Recover(context.Background()); err != nil {
		t.Fatalf("recover error: %v", err)
	}
	if e2.ActiveOrderCount() != 2 {
		t.Errorf("active count after recovery = %d, want 2", e2.ActiveOrderCount())
	}
}

func TestRecoverFromFileStore(t *testing.T) {
	path := filepath.Join(t.TempDir(), "events.log")

	store1, err := event.OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}
	e1 := NewEngine(store1, time.Second)

	e1.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer-1", 0)
	e1.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller-1", 0)
	e1.RunAuctionTickCtx(context.Background())

	if err := store1.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	// Simulate process restart
	store2, err := event.OpenFileStore(path)
	if err != nil {
		t.Fatalf("Reopen: %v", err)
	}
	t.Cleanup(func() { store2.Close() })

	e2 := NewEngine(store2, time.Second)
	if err := e2.Recover(context.Background()); err != nil {
		t.Fatalf("Recover: %v", err)
	}

	history, err := e2.GetAuctionHistory("ETH/USDC", 10)
	if err != nil {
		t.Fatalf("GetAuctionHistory: %v", err)
	}
	if len(history) != 1 {
		t.Fatalf("history len = %d, want 1", len(history))
	}
	if !history[0].MatchedVolume.Equal(decimal.NewFromInt(3)) {
		t.Errorf("matched volume = %s, want 3", history[0].MatchedVolume)
	}
}

// blockingAggregator blocks inside Aggregate until release is closed, so the
// test can check whether e.mu is held across the call.
type blockingAggregator struct {
	entered chan struct{}
	release chan struct{}
}

func (b *blockingAggregator) Aggregate(ctx context.Context, _ uuid.UUID, _ []event.OrderMatched) ([]byte, error) {
	select {
	case b.entered <- struct{}{}:
	default:
	}
	select {
	case <-b.release:
		return []byte("proof"), nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

// Aggregate runs OFF e.mu, so a slow aggregator must not block PlaceOrder.
// This is the inverse of the pre-refactor behavior: we now assert PlaceOrder
// completes while the aggregator is still blocked on its release channel.
func TestRunAuctionTick_SlowAggregatorDoesNotBlockPlaceOrder(t *testing.T) {
	e := NewEngine(event.NewMemStore(), time.Second)
	agg := &blockingAggregator{
		entered: make(chan struct{}, 1),
		release: make(chan struct{}),
	}
	e.SetAggregator(agg)

	if _, err := e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1850), decimal.NewFromInt(5), "buyer", 0); err != nil {
		t.Fatalf("seed bid: %v", err)
	}
	if _, err := e.placeOrderPlaintext("ETH/USDC", utils.Sell, decimal.NewFromInt(1800), decimal.NewFromInt(3), "seller", 0); err != nil {
		t.Fatalf("seed ask: %v", err)
	}

	tickDone := make(chan struct{})
	go func() {
		e.RunAuctionTickCtx(context.Background())
		close(tickDone)
	}()

	select {
	case <-agg.entered:
	case <-time.After(2 * time.Second):
		t.Fatal("aggregator never invoked")
	}

	placeDone := make(chan error, 1)
	go func() {
		_, err := e.placeOrderPlaintext("ETH/USDC", utils.Buy, decimal.NewFromInt(1900), decimal.NewFromInt(1), "buyer-2", 0)
		placeDone <- err
	}()

	// Aggregate still blocked; PlaceOrder must complete because e.mu is free.
	select {
	case err := <-placeDone:
		if err != nil {
			t.Fatalf("PlaceOrder during aggregator block: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("PlaceOrder stalled while aggregator blocked — mutex is still held across Aggregate")
	}

	close(agg.release)

	select {
	case <-tickDone:
	case <-time.After(2 * time.Second):
		t.Fatal("tick never completed")
	}
}
