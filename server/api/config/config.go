package config

import (
	"flag"
	"os"
	"strconv"
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

	OperatorKeyPath   string
	AggregatorBinPath string
	AggregatorTimeout time.Duration
	EthRPCURL         string
	DarkPoolAddress   string
	ChainID           uint64
	SubmitGasLimit    uint64
}

// envOr returns the value of the env var if set and the flag was left at its
// default; otherwise flagVal wins. Call AFTER flag.Parse.
func envOr(flagVal, defaultVal, envName string) string {
	if flagVal != defaultVal {
		return flagVal
	}
	if v := os.Getenv(envName); v != "" {
		return v
	}
	return flagVal
}

func envUint64(flagVal, defaultVal uint64, envName string) uint64 {
	if flagVal != defaultVal {
		return flagVal
	}
	if v := os.Getenv(envName); v != "" {
		if n, err := strconv.ParseUint(v, 10, 64); err == nil {
			return n
		}
	}
	return flagVal
}

func envDuration(flagVal, defaultVal time.Duration, envName string) time.Duration {
	if flagVal != defaultVal {
		return flagVal
	}
	if v := os.Getenv(envName); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			return d
		}
	}
	return flagVal
}

func Parse() Config {
	const (
		defaultAggTimeout   = 30 * time.Second
		defaultChainID      = uint64(0)
		defaultGas          = uint64(500_000)
	)

	grpcAddr := flag.String("grpc-addr", ":9090", "gRPC listen address")
	httpAddr := flag.String("http-addr", ":8080", "REST gateway listen address")
	auctionInterval := flag.Duration("auction-interval", 5*time.Second, "batch auction interval")
	apiKeys := flag.String("api-keys", "", "comma-separated API keys (empty = auth disabled)")
	rateLimit := flag.Float64("rate-limit", 10, "requests per second per client")
	rateBurst := flag.Float64("rate-burst", 20, "max burst size for rate limiter")
	rateStaleAfter := flag.Duration("rate-stale-after", 10*time.Minute, "evict idle rate-limit buckets after this duration")
	eventLogPath := flag.String("event-log", "", "path to durable event log file (empty = in-memory only)")

	operatorKeyPath := flag.String("operator-key", "", "path to operator ECDSA privkey (hex); empty = NoopDecrypter")
	aggregatorBin := flag.String("aggregator-bin", "", "path to Rust aggregator CLI; empty = NoopAggregator")
	aggregatorTimeout := flag.Duration("aggregator-timeout", defaultAggTimeout, "per-batch aggregator timeout")
	ethRPC := flag.String("eth-rpc", "", "Ethereum RPC URL; empty = NoopSubmitter")
	contractAddr := flag.String("contract-addr", "", "DarkPool contract address (0x...)")
	chainID := flag.Uint64("chain-id", defaultChainID, "EVM chain ID")
	gasLimit := flag.Uint64("submit-gas", defaultGas, "gas limit for submitBatch tx")

	flag.Parse()

	var keys []string
	if *apiKeys != "" {
		keys = strings.Split(*apiKeys, ",")
	}

	return Config{
		GRPCAddr:          *grpcAddr,
		HTTPAddr:          *httpAddr,
		AuctionInterval:   *auctionInterval,
		APIKeys:           keys,
		RateLimit:         *rateLimit,
		RateBurst:         *rateBurst,
		RateStaleAfter:    *rateStaleAfter,
		EventLogPath:      envOr(*eventLogPath, "", "DARKPOOL_EVENT_LOG"),
		OperatorKeyPath:   envOr(*operatorKeyPath, "", "DARKPOOL_OPERATOR_KEY"),
		AggregatorBinPath: envOr(*aggregatorBin, "", "DARKPOOL_AGGREGATOR_BIN"),
		AggregatorTimeout: envDuration(*aggregatorTimeout, defaultAggTimeout, "DARKPOOL_AGGREGATOR_TIMEOUT"),
		EthRPCURL:         envOr(*ethRPC, "", "DARKPOOL_ETH_RPC"),
		DarkPoolAddress:   envOr(*contractAddr, "", "DARKPOOL_CONTRACT_ADDR"),
		ChainID:           envUint64(*chainID, defaultChainID, "DARKPOOL_CHAIN_ID"),
		SubmitGasLimit:    envUint64(*gasLimit, defaultGas, "DARKPOOL_SUBMIT_GAS"),
	}
}
