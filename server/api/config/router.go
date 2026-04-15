package config

import (
	"github.com/darkpool-exchange/server/api"
	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
	"github.com/darkpool-exchange/server/api/middleware"
	"github.com/darkpool-exchange/server/engine"
	"google.golang.org/grpc"
)

func NewGRPCServer(eng *engine.Engine, cfg Config) *grpc.Server {
	auth := middleware.NewAuthInterceptor(cfg.APIKeys)
	rl := middleware.NewRateLimiter(cfg.RateLimit, cfg.RateBurst)

	srv := grpc.NewServer(
		grpc.ChainUnaryInterceptor(auth.Unary(), rl.Unary()),
		grpc.ChainStreamInterceptor(auth.Stream(), rl.Stream()),
	)

	darkpoolv1.RegisterDarkPoolServiceServer(srv, api.NewServer(eng))
	return srv
}
