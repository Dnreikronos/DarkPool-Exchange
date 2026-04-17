package config

import (
	"flag"
	"strings"
	"time"
)

type Config struct {
	GRPCAddr        string
	HTTPAddr        string
	AuctionInterval time.Duration
	APIKeys         []string
	RateLimit       float64
	RateBurst       float64
	RateStaleAfter  time.Duration
	EventLogPath    string
}

func Parse() Config {
	grpcAddr := flag.String("grpc-addr", ":9090", "gRPC listen address")
	httpAddr := flag.String("http-addr", ":8080", "REST gateway listen address")
	auctionInterval := flag.Duration("auction-interval", 5*time.Second, "batch auction interval")
	apiKeys := flag.String("api-keys", "", "comma-separated API keys (empty = auth disabled)")
	rateLimit := flag.Float64("rate-limit", 10, "requests per second per client")
	rateBurst := flag.Float64("rate-burst", 20, "max burst size for rate limiter")
	rateStaleAfter := flag.Duration("rate-stale-after", 10*time.Minute, "evict idle rate-limit buckets after this duration")
	eventLogPath := flag.String("event-log", "", "path to durable event log file (empty = in-memory only)")
	flag.Parse()

	var keys []string
	if *apiKeys != "" {
		keys = strings.Split(*apiKeys, ",")
	}

	return Config{
		GRPCAddr:        *grpcAddr,
		HTTPAddr:        *httpAddr,
		AuctionInterval: *auctionInterval,
		APIKeys:         keys,
		RateLimit:       *rateLimit,
		RateBurst:       *rateBurst,
		RateStaleAfter:  *rateStaleAfter,
		EventLogPath:    *eventLogPath,
	}
}
