package handler

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"testing"
	"time"

	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
	apiutils "github.com/darkpool-exchange/server/api/utils"
	"github.com/darkpool-exchange/server/engine/core"
	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

func newTestServer() *Server {
	eng := core.NewEngine(event.NewMemStore(), time.Second)
	return NewServer(eng)
}

var testKeyCounter uint64

// buildReq encodes a DecryptedOrder with the noop decrypter's JSON format and
// computes the matching stub (sha256) commitment. Tests that want to exercise
// the commitment-mismatch path construct the request manually instead.
func buildReq(t *testing.T, d core.DecryptedOrder) *darkpoolv1.PlaceOrderRequest {
	t.Helper()
	ct, err := json.Marshal(d)
	if err != nil {
		t.Fatalf("marshal DecryptedOrder: %v", err)
	}
	return &darkpoolv1.PlaceOrderRequest{
		Commitment:       core.ComputeCommitment(d),
		Proof:            testProof(),
		EncryptedPayload: ct,
	}
}

func sideFromProto(s darkpoolv1.Side) utils.Side {
	if s == darkpoolv1.Side_SIDE_SELL {
		return utils.Sell
	}
	return utils.Buy
}

func placeTestOrder(t *testing.T, srv *Server, pair string, side darkpoolv1.Side, price, size string) string {
	t.Helper()
	testKeyCounter++
	d := core.DecryptedOrder{
		Pair:          pair,
		Side:          sideFromProto(side),
		Price:         decimal.RequireFromString(price),
		Size:          decimal.RequireFromString(size),
		CommitmentKey: fmt.Sprintf("test-key-%d", testKeyCounter),
		TTL:           600 * time.Second,
	}
	resp, err := srv.PlaceOrder(context.Background(), buildReq(t, d))
	if err != nil {
		t.Fatalf("placeTestOrder: %v", err)
	}
	return resp.Order.Id
}

func assertCode(t *testing.T, err error, want codes.Code) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected error with code %v, got nil", want)
	}
	st, ok := status.FromError(err)
	if !ok {
		t.Fatalf("expected gRPC status error, got %T: %v", err, err)
	}
	if st.Code() != want {
		t.Errorf("code = %v, want %v (msg: %s)", st.Code(), want, st.Message())
	}
}

// mockAuctionStream implements DarkPoolService_StreamAuctionsServer.
type mockAuctionStream struct {
	grpc.ServerStream
	ctx    context.Context
	mu     sync.Mutex
	events []*darkpoolv1.AuctionEvent
}

func (m *mockAuctionStream) Send(e *darkpoolv1.AuctionEvent) error {
	m.mu.Lock()
	m.events = append(m.events, e)
	m.mu.Unlock()
	return nil
}

func (m *mockAuctionStream) Context() context.Context { return m.ctx }

// SetHeader/SendHeader/SetTrailer use embedded defaults (panic on call, but
// the handler never calls them so this is safe for tests).

// ---------------------------------------------------------------------------
// PlaceOrder
// ---------------------------------------------------------------------------

func validDecrypted() core.DecryptedOrder {
	return core.DecryptedOrder{
		Pair:          "ETH/USDC",
		Side:          utils.Buy,
		Price:         decimal.RequireFromString("1800.50"),
		Size:          decimal.NewFromInt(10),
		CommitmentKey: "ck-1",
		TTL:           60 * time.Second,
	}
}

func TestPlaceOrder_Success(t *testing.T) {
	srv := newTestServer()
	resp, err := srv.PlaceOrder(context.Background(), buildReq(t, validDecrypted()))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	o := resp.Order
	if o == nil {
		t.Fatal("order is nil")
	}
	if o.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", o.Pair)
	}
	if o.Side != darkpoolv1.Side_SIDE_BUY {
		t.Errorf("side = %v, want SIDE_BUY", o.Side)
	}
	if o.Price != "1800.5" && o.Price != "1800.50" {
		t.Errorf("price = %s, want 1800.50", o.Price)
	}
	if _, err := uuid.Parse(o.Id); err != nil {
		t.Errorf("id %q is not a valid UUID: %v", o.Id, err)
	}
}

func TestPlaceOrder_ValidationErrors(t *testing.T) {
	tests := []struct {
		name string
		mut  func(*core.DecryptedOrder)
	}{
		{"empty pair", func(d *core.DecryptedOrder) { d.Pair = "" }},
		{"zero price", func(d *core.DecryptedOrder) { d.Price = decimal.Zero }},
		{"negative size", func(d *core.DecryptedOrder) { d.Size = decimal.NewFromInt(-1) }},
		{"empty commitment key", func(d *core.DecryptedOrder) { d.CommitmentKey = "" }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			srv := newTestServer()
			d := validDecrypted()
			tt.mut(&d)
			_, err := srv.PlaceOrder(context.Background(), buildReq(t, d))
			assertCode(t, err, codes.InvalidArgument)
		})
	}
}

func TestPlaceOrder_MissingCommitment(t *testing.T) {
	srv := newTestServer()
	req := buildReq(t, validDecrypted())
	req.Commitment = nil
	_, err := srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
	if st, _ := status.FromError(err); st.Message() != apiutils.MsgCommitmentRequired {
		t.Errorf("msg = %q, want %q", st.Message(), apiutils.MsgCommitmentRequired)
	}
}

func TestPlaceOrder_MissingCiphertext(t *testing.T) {
	srv := newTestServer()
	req := buildReq(t, validDecrypted())
	req.EncryptedPayload = nil
	_, err := srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
	if st, _ := status.FromError(err); st.Message() != apiutils.MsgCiphertextRequired {
		t.Errorf("msg = %q, want %q", st.Message(), apiutils.MsgCiphertextRequired)
	}
}

func TestPlaceOrder_CiphertextTooLarge(t *testing.T) {
	srv := newTestServer()
	req := buildReq(t, validDecrypted())
	req.EncryptedPayload = make([]byte, apiutils.MaxCiphertextBytes+1)
	_, err := srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
	if st, _ := status.FromError(err); st.Message() != apiutils.MsgCiphertextTooLarge {
		t.Errorf("msg = %q, want %q", st.Message(), apiutils.MsgCiphertextTooLarge)
	}
}

// TestPlaceOrder_CommitmentMismatch is the canary test for the entire
// encrypt-only spec. A ciphertext that decrypts to one order paired with a
// commitment that hashes a different order must be rejected. If this fails,
// a client could submit a proof of order A with plaintext of order B.
func TestPlaceOrder_CommitmentMismatch(t *testing.T) {
	srv := newTestServer()
	honest := validDecrypted()
	attacker := honest
	attacker.Price = decimal.NewFromInt(9999)

	ct, err := json.Marshal(honest)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	req := &darkpoolv1.PlaceOrderRequest{
		Commitment:       core.ComputeCommitment(attacker),
		Proof:            testProof(),
		EncryptedPayload: ct,
	}

	_, err = srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
	if st, _ := status.FromError(err); st.Message() != apiutils.MsgCommitmentMismatch {
		t.Errorf("msg = %q, want %q", st.Message(), apiutils.MsgCommitmentMismatch)
	}
}

func TestPlaceOrder_MissingProof(t *testing.T) {
	srv := newTestServer()
	req := buildReq(t, validDecrypted())
	req.Proof = nil
	_, err := srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
}

func TestPlaceOrder_ProofTooLarge(t *testing.T) {
	srv := newTestServer()
	req := buildReq(t, validDecrypted())
	req.Proof = make([]byte, apiutils.MaxProofBytes+1)
	_, err := srv.PlaceOrder(context.Background(), req)
	assertCode(t, err, codes.InvalidArgument)
	if st, _ := status.FromError(err); st.Message() != apiutils.MsgProofTooLarge {
		t.Errorf("msg = %q, want %q", st.Message(), apiutils.MsgProofTooLarge)
	}
}

func testProof() []byte {
	// placeholder bytes — passes format check until real circuits land
	return []byte("stub-proof")
}

// ---------------------------------------------------------------------------
// CancelOrder
// ---------------------------------------------------------------------------

func TestCancelOrder_Success(t *testing.T) {
	srv := newTestServer()
	id := placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1800", "5")
	_, err := srv.CancelOrder(context.Background(), &darkpoolv1.CancelOrderRequest{
		OrderId: id,
		Reason:  "testing",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestCancelOrder_InvalidUUID(t *testing.T) {
	srv := newTestServer()
	_, err := srv.CancelOrder(context.Background(), &darkpoolv1.CancelOrderRequest{
		OrderId: "not-a-uuid",
	})
	assertCode(t, err, codes.InvalidArgument)
}

func TestCancelOrder_NotFound(t *testing.T) {
	srv := newTestServer()
	_, err := srv.CancelOrder(context.Background(), &darkpoolv1.CancelOrderRequest{
		OrderId: uuid.New().String(),
	})
	assertCode(t, err, codes.NotFound)
}

// ---------------------------------------------------------------------------
// GetOrder
// ---------------------------------------------------------------------------

func TestGetOrder_Success(t *testing.T) {
	srv := newTestServer()
	id := placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1800", "5")
	resp, err := srv.GetOrder(context.Background(), &darkpoolv1.GetOrderRequest{OrderId: id})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Order.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", resp.Order.Pair)
	}
	if resp.Order.Side != darkpoolv1.Side_SIDE_BUY {
		t.Errorf("side = %v, want SIDE_BUY", resp.Order.Side)
	}
}

func TestGetOrder_InvalidUUID(t *testing.T) {
	srv := newTestServer()
	_, err := srv.GetOrder(context.Background(), &darkpoolv1.GetOrderRequest{OrderId: "bad"})
	assertCode(t, err, codes.InvalidArgument)
}

func TestGetOrder_NotFound(t *testing.T) {
	srv := newTestServer()
	_, err := srv.GetOrder(context.Background(), &darkpoolv1.GetOrderRequest{OrderId: uuid.New().String()})
	assertCode(t, err, codes.NotFound)
}

// ---------------------------------------------------------------------------
// GetOrderBook
// ---------------------------------------------------------------------------

func TestGetOrderBook_Success(t *testing.T) {
	srv := newTestServer()
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1800", "5")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1790", "3")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_SELL, "1850", "2")

	resp, err := srv.GetOrderBook(context.Background(), &darkpoolv1.GetOrderBookRequest{Pair: "ETH/USDC"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", resp.Pair)
	}
	if len(resp.Bids) != 2 {
		t.Errorf("bids levels = %d, want 2", len(resp.Bids))
	}
	if len(resp.Asks) != 1 {
		t.Errorf("asks levels = %d, want 1", len(resp.Asks))
	}
}

func TestGetOrderBook_EmptyPair(t *testing.T) {
	srv := newTestServer()
	_, err := srv.GetOrderBook(context.Background(), &darkpoolv1.GetOrderBookRequest{Pair: ""})
	assertCode(t, err, codes.InvalidArgument)
}

func TestGetOrderBook_Aggregation(t *testing.T) {
	srv := newTestServer()
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "100", "5")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "100", "3")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "200", "10")

	resp, err := srv.GetOrderBook(context.Background(), &darkpoolv1.GetOrderBookRequest{Pair: "ETH/USDC"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(resp.Bids) != 2 {
		t.Fatalf("bids levels = %d, want 2", len(resp.Bids))
	}
	found100 := false
	for _, lvl := range resp.Bids {
		if lvl.Price == "100" {
			found100 = true
			if lvl.TotalSize != "8" {
				t.Errorf("total size at 100 = %s, want 8", lvl.TotalSize)
			}
			if lvl.OrderCount != 2 {
				t.Errorf("order count at 100 = %d, want 2", lvl.OrderCount)
			}
		}
	}
	if !found100 {
		t.Error("price level 100 not found in bids")
	}
}

// ---------------------------------------------------------------------------
// GetAuctionHistory
// ---------------------------------------------------------------------------

func TestGetAuctionHistory_Empty(t *testing.T) {
	srv := newTestServer()
	resp, err := srv.GetAuctionHistory(context.Background(), &darkpoolv1.GetAuctionHistoryRequest{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(resp.Auctions) != 0 {
		t.Errorf("auctions = %d, want 0", len(resp.Auctions))
	}
}

func TestGetAuctionHistory_WithAuction(t *testing.T) {
	srv := newTestServer()
	// place crossing orders: buy at 1850, sell at 1800
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1850", "5")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_SELL, "1800", "5")

	srv.engine.RunAuctionTickCtx(context.Background())

	resp, err := srv.GetAuctionHistory(context.Background(), &darkpoolv1.GetAuctionHistoryRequest{
		Pair: "ETH/USDC",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(resp.Auctions) != 1 {
		t.Fatalf("auctions = %d, want 1", len(resp.Auctions))
	}
	a := resp.Auctions[0]
	if a.Pair != "ETH/USDC" {
		t.Errorf("pair = %s, want ETH/USDC", a.Pair)
	}
	if a.MatchCount < 1 {
		t.Errorf("match_count = %d, want >= 1", a.MatchCount)
	}
}

// ---------------------------------------------------------------------------
// StreamAuctions
// ---------------------------------------------------------------------------

func TestStreamAuctions_ReceivesEvent(t *testing.T) {
	srv := newTestServer()
	ctx, cancel := context.WithCancel(context.Background())
	// add metadata so interceptors don't complain if present
	ctx = metadata.NewIncomingContext(ctx, metadata.MD{})

	stream := &mockAuctionStream{ctx: ctx}

	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1850", "5")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_SELL, "1800", "5")

	done := make(chan error, 1)
	go func() {
		done <- srv.StreamAuctions(&darkpoolv1.StreamAuctionsRequest{}, stream)
	}()

	// let goroutine subscribe before running auction
	time.Sleep(20 * time.Millisecond)

	srv.engine.RunAuctionTickCtx(context.Background())

	// let stream loop receive the notification
	time.Sleep(50 * time.Millisecond)
	cancel()

	if err := <-done; err != nil {
		t.Fatalf("StreamAuctions returned error: %v", err)
	}

	stream.mu.Lock()
	count := len(stream.events)
	stream.mu.Unlock()

	if count < 1 {
		t.Errorf("received %d events, want >= 1", count)
	}
}

func TestStreamAuctions_FiltersByPair(t *testing.T) {
	srv := newTestServer()
	ctx, cancel := context.WithCancel(context.Background())
	ctx = metadata.NewIncomingContext(ctx, metadata.MD{})
	stream := &mockAuctionStream{ctx: ctx}

	// create auctions for two pairs
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_BUY, "1850", "5")
	placeTestOrder(t, srv, "ETH/USDC", darkpoolv1.Side_SIDE_SELL, "1800", "5")
	placeTestOrder(t, srv, "BTC/USDC", darkpoolv1.Side_SIDE_BUY, "60000", "1")
	placeTestOrder(t, srv, "BTC/USDC", darkpoolv1.Side_SIDE_SELL, "59000", "1")

	done := make(chan error, 1)
	go func() {
		done <- srv.StreamAuctions(&darkpoolv1.StreamAuctionsRequest{Pair: "ETH/USDC"}, stream)
	}()

	time.Sleep(20 * time.Millisecond)
	srv.engine.RunAuctionTickCtx(context.Background())
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	stream.mu.Lock()
	defer stream.mu.Unlock()

	for _, e := range stream.events {
		if e.Pair != "ETH/USDC" {
			t.Errorf("received event for pair %s, want only ETH/USDC", e.Pair)
		}
	}
}
