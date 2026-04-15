package config

import (
	"context"
	"time"

	"github.com/darkpool-exchange/server/api/handler"
	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
	"github.com/darkpool-exchange/server/api/middleware"
	"github.com/darkpool-exchange/server/engine/core"
	"google.golang.org/grpc"
)

func NewGRPCServer(ctx context.Context, eng *core.Engine, cfg Config) *grpc.Server {
	auth := middleware.NewAuthInterceptor(cfg.APIKeys)
	rl := middleware.NewRateLimiter(cfg.RateLimit, cfg.RateBurst)
	rl.StartCleanup(ctx, time.Minute)

	srv := grpc.NewServer(
		grpc.ChainUnaryInterceptor(auth.Unary(), rl.Unary()),
		grpc.ChainStreamInterceptor(auth.Stream(), rl.Stream()),
	)

	darkpoolv1.RegisterDarkPoolServiceServer(srv, handler.NewServer(eng))
	return srv
}
