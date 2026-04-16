package middleware

import (
	"context"
	"net"
	"testing"
	"time"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/peer"
)

// ---------------------------------------------------------------------------
// allow() logic
// ---------------------------------------------------------------------------

func TestRateLimit_AllowsWithinBurst(t *testing.T) {
	rl := NewRateLimiter(10, 5, time.Minute)
	ctx := ctxWithAPIKey("client-1")

	for i := 0; i < 5; i++ {
		if err := rl.allow(ctx); err != nil {
			t.Fatalf("request %d: unexpected error: %v", i+1, err)
		}
	}
}

func TestRateLimit_BlocksWhenExhausted(t *testing.T) {
	rl := NewRateLimiter(10, 3, time.Minute)
	ctx := ctxWithAPIKey("client-1")

	for i := 0; i < 3; i++ {
		if err := rl.allow(ctx); err != nil {
			t.Fatalf("request %d should pass: %v", i+1, err)
		}
	}
	err := rl.allow(ctx)
	assertCode(t, err, codes.ResourceExhausted)
}

func TestRateLimit_RefillsOverTime(t *testing.T) {
	rl := NewRateLimiter(10, 2, time.Minute)
	ctx := ctxWithAPIKey("client-1")

	// exhaust both tokens
	rl.allow(ctx)
	rl.allow(ctx)

	// simulate 200ms passing (10/sec * 0.2s = 2 tokens refilled)
	rl.mu.Lock()
	rl.buckets["client-1"].lastFill = time.Now().Add(-200 * time.Millisecond)
	rl.mu.Unlock()

	if err := rl.allow(ctx); err != nil {
		t.Fatalf("expected refill to allow request: %v", err)
	}
}

func TestRateLimit_TokensCappedAtCapacity(t *testing.T) {
	rl := NewRateLimiter(1000, 3, time.Minute)
	ctx := ctxWithAPIKey("client-1")

	// use 1 token
	rl.allow(ctx)

	// simulate a long time passing (tokens should cap at 3)
	rl.mu.Lock()
	rl.buckets["client-1"].lastFill = time.Now().Add(-10 * time.Second)
	rl.mu.Unlock()

	// should be able to use 3 (cap), then fail on 4th
	for i := 0; i < 3; i++ {
		if err := rl.allow(ctx); err != nil {
			t.Fatalf("request %d should pass after refill: %v", i+1, err)
		}
	}
	err := rl.allow(ctx)
	assertCode(t, err, codes.ResourceExhausted)
}

// ---------------------------------------------------------------------------
// Interceptors
// ---------------------------------------------------------------------------

func TestRateLimit_Unary_Allowed(t *testing.T) {
	rl := NewRateLimiter(10, 10, time.Minute)
	interceptor := rl.Unary()
	ctx := ctxWithAPIKey("client-1")

	resp, err := interceptor(ctx, nil, nil, dummyUnaryHandler)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp != "ok" {
		t.Errorf("response = %v, want ok", resp)
	}
}

func TestRateLimit_Unary_Blocked(t *testing.T) {
	rl := NewRateLimiter(10, 1, time.Minute)
	interceptor := rl.Unary()
	ctx := ctxWithAPIKey("client-1")

	// first passes
	if _, err := interceptor(ctx, nil, nil, dummyUnaryHandler); err != nil {
		t.Fatalf("first request should pass: %v", err)
	}
	// second blocked
	_, err := interceptor(ctx, nil, nil, dummyUnaryHandler)
	assertCode(t, err, codes.ResourceExhausted)
}

func TestRateLimit_Stream_Allowed(t *testing.T) {
	rl := NewRateLimiter(10, 10, time.Minute)
	interceptor := rl.Stream()
	ss := &mockServerStream{ctx: ctxWithAPIKey("client-1")}

	err := interceptor(nil, ss, nil, dummyStreamHandler)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRateLimit_Stream_Blocked(t *testing.T) {
	rl := NewRateLimiter(10, 1, time.Minute)
	interceptor := rl.Stream()

	ss := &mockServerStream{ctx: ctxWithAPIKey("client-1")}
	// first passes
	if err := interceptor(nil, ss, nil, dummyStreamHandler); err != nil {
		t.Fatalf("first request should pass: %v", err)
	}
	// second blocked
	err := interceptor(nil, ss, nil, dummyStreamHandler)
	assertCode(t, err, codes.ResourceExhausted)
}

// ---------------------------------------------------------------------------
// clientKey
// ---------------------------------------------------------------------------

func ctxWithPeer(ip string, port int) context.Context {
	return peer.NewContext(context.Background(), &peer.Peer{
		Addr: &net.TCPAddr{IP: net.ParseIP(ip), Port: port},
	})
}

func TestClientKey_APIKey(t *testing.T) {
	got := clientKey(ctxWithAPIKey("my-key"))
	if got != "my-key" {
		t.Errorf("clientKey = %q, want my-key", got)
	}
}

func TestClientKey_PeerAddr(t *testing.T) {
	got := clientKey(ctxWithPeer("192.168.1.1", 5000))
	if got != "192.168.1.1" {
		t.Errorf("clientKey = %q, want 192.168.1.1", got)
	}
}

func TestClientKey_Anonymous(t *testing.T) {
	got := clientKey(context.Background())
	if got != "anonymous" {
		t.Errorf("clientKey = %q, want anonymous", got)
	}
}

func TestClientKey_APIKeyPrecedence(t *testing.T) {
	// context with both API key and peer
	ctx := ctxWithAPIKey("api-key-1")
	ctx = peer.NewContext(ctx, &peer.Peer{
		Addr: &net.TCPAddr{IP: net.ParseIP("10.0.0.1"), Port: 1234},
	})
	got := clientKey(ctx)
	if got != "api-key-1" {
		t.Errorf("clientKey = %q, want api-key-1", got)
	}
}

// ---------------------------------------------------------------------------
// Separate buckets per client
// ---------------------------------------------------------------------------

func TestRateLimit_SeparateBuckets(t *testing.T) {
	rl := NewRateLimiter(10, 2, time.Minute)

	ctxA := ctxWithAPIKey("client-a")
	ctxB := ctxWithAPIKey("client-b")

	// exhaust client-a
	rl.allow(ctxA)
	rl.allow(ctxA)
	err := rl.allow(ctxA)
	assertCode(t, err, codes.ResourceExhausted)

	// client-b should still have tokens
	if err := rl.allow(ctxB); err != nil {
		t.Fatalf("client-b should have tokens: %v", err)
	}
}

// ---------------------------------------------------------------------------
// Eviction
// ---------------------------------------------------------------------------

func TestRateLimit_EvictStale(t *testing.T) {
	rl := NewRateLimiter(10, 5, 100*time.Millisecond)
	ctx := ctxWithAPIKey("ephemeral")

	rl.allow(ctx)

	// make the bucket stale
	rl.mu.Lock()
	rl.buckets["ephemeral"].lastFill = time.Now().Add(-time.Second)
	rl.mu.Unlock()

	rl.evictStale(time.Now())

	rl.mu.Lock()
	count := len(rl.buckets)
	rl.mu.Unlock()

	if count != 0 {
		t.Errorf("buckets remaining = %d, want 0", count)
	}
}

func TestRateLimit_EvictStale_KeepsFresh(t *testing.T) {
	rl := NewRateLimiter(10, 5, time.Minute)

	rl.allow(ctxWithAPIKey("fresh"))
	rl.allow(ctxWithAPIKey("stale"))

	// make only "stale" bucket old
	rl.mu.Lock()
	rl.buckets["stale"].lastFill = time.Now().Add(-2 * time.Minute)
	rl.mu.Unlock()

	rl.evictStale(time.Now())

	rl.mu.Lock()
	_, hasFresh := rl.buckets["fresh"]
	_, hasStale := rl.buckets["stale"]
	rl.mu.Unlock()

	if !hasFresh {
		t.Error("fresh bucket was evicted")
	}
	if hasStale {
		t.Error("stale bucket was not evicted")
	}
}

func TestRateLimit_DefaultStaleAfter(t *testing.T) {
	rl := NewRateLimiter(10, 5, 0)
	if rl.staleAfter != 10*time.Minute {
		t.Errorf("staleAfter = %v, want 10m", rl.staleAfter)
	}
}
