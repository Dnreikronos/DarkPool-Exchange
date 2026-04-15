package middleware

import (
	"context"
	"sync"
	"time"

	apiutils "github.com/darkpool-exchange/server/api/utils"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

type bucket struct {
	tokens   float64
	lastFill time.Time
}

type RateLimiter struct {
	mu         sync.Mutex
	buckets    map[string]*bucket
	rate       float64
	capacity   float64
	staleAfter time.Duration
}

func NewRateLimiter(ratePerSecond, burst float64) *RateLimiter {
	return &RateLimiter{
		buckets:    make(map[string]*bucket),
		rate:       ratePerSecond,
		capacity:   burst,
		staleAfter: 10 * time.Minute,
	}
}

func (r *RateLimiter) Unary() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req any, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (any, error) {
		if err := r.allow(ctx); err != nil {
			return nil, err
		}
		return handler(ctx, req)
	}
}

func (r *RateLimiter) Stream() grpc.StreamServerInterceptor {
	return func(srv any, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
		if err := r.allow(ss.Context()); err != nil {
			return err
		}
		return handler(srv, ss)
	}
}

func (r *RateLimiter) allow(ctx context.Context) error {
	key := clientKey(ctx)

	r.mu.Lock()
	defer r.mu.Unlock()

	b, ok := r.buckets[key]
	if !ok {
		b = &bucket{tokens: r.capacity, lastFill: time.Now()}
		r.buckets[key] = b
	}

	now := time.Now()
	elapsed := now.Sub(b.lastFill).Seconds()
	b.tokens += elapsed * r.rate
	if b.tokens > r.capacity {
		b.tokens = r.capacity
	}
	b.lastFill = now

	if b.tokens < 1 {
		return status.Error(codes.ResourceExhausted, apiutils.MsgRateLimitExceeded)
	}

	b.tokens--
	return nil
}

func (r *RateLimiter) StartCleanup(ctx context.Context, interval time.Duration) {
	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case now := <-ticker.C:
				r.evictStale(now)
			}
		}
	}()
}

func (r *RateLimiter) evictStale(now time.Time) {
	r.mu.Lock()
	defer r.mu.Unlock()
	for key, b := range r.buckets {
		if now.Sub(b.lastFill) > r.staleAfter {
			delete(r.buckets, key)
		}
	}
}

func clientKey(ctx context.Context) string {
	md, ok := metadata.FromIncomingContext(ctx)
	if ok {
		keys := md.Get(authHeader)
		if len(keys) > 0 {
			return keys[0]
		}
	}
	return "anonymous"
}
