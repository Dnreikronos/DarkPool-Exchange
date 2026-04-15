package main

import (
	"context"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/darkpool-exchange/server/api"
	"github.com/darkpool-exchange/server/api/config"
	"github.com/darkpool-exchange/server/engine"
	"github.com/darkpool-exchange/server/engine/event"
)

func main() {
	cfg := config.Parse()

	store := event.NewMemStore()
	eng := engine.NewEngine(store, cfg.AuctionInterval)

	if err := eng.Recover(); err != nil {
		log.Fatalf("engine recovery failed: %v", err)
	}

	grpcServer := config.NewGRPCServer(eng, cfg)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go eng.Start(ctx)

	lis, err := net.Listen("tcp", cfg.GRPCAddr)
	if err != nil {
		log.Fatalf("failed to listen on %s: %v", cfg.GRPCAddr, err)
	}

	go func() {
		log.Printf("gRPC server listening on %s", cfg.GRPCAddr)
		if err := grpcServer.Serve(lis); err != nil {
			log.Fatalf("gRPC server failed: %v", err)
		}
	}()

	gw, err := api.NewGateway(ctx, cfg.GRPCAddr)
	if err != nil {
		log.Fatalf("failed to create gateway: %v", err)
	}

	httpServer := &http.Server{
		Addr:    cfg.HTTPAddr,
		Handler: gw,
	}

	go func() {
		log.Printf("REST gateway listening on %s", cfg.HTTPAddr)
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("REST gateway failed: %v", err)
		}
	}()

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
