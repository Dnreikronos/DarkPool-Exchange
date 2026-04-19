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

	"github.com/darkpool-exchange/server/api/config"
	"github.com/darkpool-exchange/server/api/gateway"
	"github.com/darkpool-exchange/server/engine/core"
	"github.com/darkpool-exchange/server/engine/event"

	"github.com/ethereum/go-ethereum/common"
)

func main() {
	cfg := config.Parse()

	var store event.Store
	if cfg.EventLogPath != "" {
		fs, err := event.OpenFileStore(cfg.EventLogPath)
		if err != nil {
			log.Fatalf("failed to open event log %s: %v", cfg.EventLogPath, err)
		}
		defer fs.Close()
		store = fs
		log.Printf("event log: %s (durable)", cfg.EventLogPath)
	} else {
		store = event.NewMemStore()
		log.Printf("event log: in-memory (not durable)")
	}

	eng := core.NewEngine(store, cfg.AuctionInterval)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	if cfg.OperatorKeyPath != "" {
		dec, err := core.NewECIESDecrypterFromFile(cfg.OperatorKeyPath)
		if err != nil {
			log.Fatalf("load operator key: %v", err)
		}
		eng.SetDecrypter(dec)
		log.Printf("decrypter: ECIES (operator key %s)", cfg.OperatorKeyPath)
	} else {
		log.Printf("decrypter: noop (set -operator-key to enable ECIES)")
	}

	if cfg.AggregatorBinPath != "" {
		agg, err := core.NewSubprocessAggregator(cfg.AggregatorBinPath, cfg.AggregatorTimeout)
		if err != nil {
			log.Fatalf("init aggregator: %v", err)
		}
		eng.SetAggregator(agg)
		log.Printf("aggregator: subprocess %s (timeout %s)", cfg.AggregatorBinPath, cfg.AggregatorTimeout)
	} else {
		log.Printf("aggregator: noop (set -aggregator-bin to enable)")
	}

	var watcher *core.SettlementWatcher
	if cfg.EthRPCURL != "" {
		sub, err := core.NewEthSubmitter(ctx, core.EthSubmitterConfig{
			RPCURL:          cfg.EthRPCURL,
			OperatorKeyPath: cfg.OperatorKeyPath,
			ContractAddress: cfg.DarkPoolAddress,
			ChainID:         cfg.ChainID,
			GasLimit:        cfg.SubmitGasLimit,
		})
		if err != nil {
			log.Fatalf("init eth submitter: %v", err)
		}
		eng.SetSubmitter(sub)
		log.Printf("batch submitter: eth rpc=%s contract=%s", cfg.EthRPCURL, cfg.DarkPoolAddress)

		watcher, err = core.NewSettlementWatcher(sub.Client(), common.HexToAddress(cfg.DarkPoolAddress), store, eng)
		if err != nil {
			log.Fatalf("init settlement watcher: %v", err)
		}
	} else {
		log.Printf("batch submitter: noop (set -eth-rpc to enable on-chain settlement)")
	}

	if err := eng.Recover(ctx); err != nil {
		log.Fatalf("engine recovery failed: %v", err)
	}

	grpcServer := config.NewGRPCServer(ctx, eng, cfg)

	engDone := make(chan struct{})
	go func() {
		eng.Start(ctx)
		close(engDone)
	}()

	if watcher != nil {
		go watcher.Run(ctx)
	}

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

	gw, err := gateway.NewGateway(ctx, cfg.GRPCAddr)
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

	// Drain engine loop before returning so deferred fs.Close can't race an in-flight tick.
	<-engDone

	log.Println("shutdown complete")
}
