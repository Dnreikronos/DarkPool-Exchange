package main

import (
	"context"
	"flag"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/darkpool-exchange/server/api"
	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
	"github.com/darkpool-exchange/server/api/middleware"
	engine "github.com/darkpool-exchange/server/engine"
	"github.com/darkpool-exchange/server/engine/event"
	"google.golang.org/grpc"
)

func main() {
	grpcAddr := flag.String("grpc-addr", ":9090", "gRPC listen address")
	httpAddr := flag.String("http-addr", ":8080", "REST gateway listen address")
	auctionInterval := flag.Duration("auction-interval", 5*time.Second, "batch auction interval")
	apiKeys := flag.String("api-keys", "", "comma-separated API keys (empty = auth disabled)")
	rateLimit := flag.Float64("rate-limit", 10, "requests per second per client")
	rateBurst := flag.Float64("rate-burst", 20, "max burst size for rate limiter")
	flag.Parse()

	store := event.NewMemStore()
	eng := engine.NewEngine(store, *auctionInterval)

	if err := eng.Recover(); err != nil {
		log.Fatalf("engine recovery failed: %v", err)
	}

	var keys []string
	if *apiKeys != "" {
		keys = strings.Split(*apiKeys, ",")
	}
	auth := middleware.NewAuthInterceptor(keys)
	rl := middleware.NewRateLimiter(*rateLimit, *rateBurst)

	grpcServer := grpc.NewServer(
		grpc.ChainUnaryInterceptor(auth.Unary(), rl.Unary()),
		grpc.ChainStreamInterceptor(auth.Stream(), rl.Stream()),
	)

	srv := api.NewServer(eng)
	darkpoolv1.RegisterDarkPoolServiceServer(grpcServer, srv)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Start auction ticker
	go eng.Start(ctx)

	// Start gRPC server
	lis, err := net.Listen("tcp", *grpcAddr)
	if err != nil {
		log.Fatalf("failed to listen on %s: %v", *grpcAddr, err)
	}

	go func() {
		log.Printf("gRPC server listening on %s", *grpcAddr)
		if err := grpcServer.Serve(lis); err != nil {
			log.Fatalf("gRPC server failed: %v", err)
		}
	}()

	// Start REST gateway
	gw, err := api.NewGateway(ctx, *grpcAddr)
	if err != nil {
		log.Fatalf("failed to create gateway: %v", err)
	}

	httpServer := &http.Server{
		Addr:    *httpAddr,
		Handler: gw,
	}

	go func() {
		log.Printf("REST gateway listening on %s", *httpAddr)
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("REST gateway failed: %v", err)
		}
	}()

	// Graceful shutdown
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	<-sigCh

	log.Println("shutting down...")
	cancel()
	grpcServer.GracefulStop()

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer shutdownCancel()
	httpServer.Shutdown(shutdownCtx)

	log.Println("shutdown complete")
}
