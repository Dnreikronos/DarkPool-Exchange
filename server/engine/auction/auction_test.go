package auction

import (
	"testing"
	"time"

	"github.com/darkpool-exchange/server/engine/model"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

func newOrder(side utils.Side, price, size int64) model.Order {
	return model.Order{
		ID:            uuid.New(),
		Pair:          "ETH/USDC",
		Side:          side,
		Price:         decimal.NewFromInt(price),
		Size:          decimal.NewFromInt(size),
		RemainingSize: decimal.NewFromInt(size),
		CommitmentKey: uuid.NewString(),
		SubmittedAt:   time.Now(),
		ExpiresAt:     time.Now().Add(10 * time.Minute),
	}
}

func TestRun_BasicMatch(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1790, 10)}

	result := Run("ETH/USDC", bids, asks)
	if result == nil {
		t.Fatal("expected auction result, got nil")
	}
	if len(result.Matches) != 1 {
		t.Fatalf("matches = %d, want 1", len(result.Matches))
	}
	if !result.MatchedVolume.Equal(decimal.NewFromInt(10)) {
		t.Fatalf("volume = %s, want 10", result.MatchedVolume)
	}
}

func TestRun_NoCrossing(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1700, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1800, 10)}

	result := Run("ETH/USDC", bids, asks)
	if result != nil {
		t.Fatal("expected nil result when no crossing")
	}
}

func TestRun_PartialFill(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1790, 4)}

	result := Run("ETH/USDC", bids, asks)
	if result == nil {
		t.Fatal("expected result")
	}
	if !result.MatchedVolume.Equal(decimal.NewFromInt(4)) {
		t.Fatalf("volume = %s, want 4", result.MatchedVolume)
	}
}

func TestRun_MultipleBidsAndAsks(t *testing.T) {
	bids := []model.Order{
		newOrder(utils.Buy, 1810, 5),
		newOrder(utils.Buy, 1800, 10),
		newOrder(utils.Buy, 1795, 8),
	}
	asks := []model.Order{
		newOrder(utils.Sell, 1785, 6),
		newOrder(utils.Sell, 1790, 4),
		newOrder(utils.Sell, 1800, 10),
	}

	result := Run("ETH/USDC", bids, asks)
	if result == nil {
		t.Fatal("expected result")
	}
	if !result.ClearingPrice.IsPositive() {
		t.Fatalf("clearing price = %s, want > 0", result.ClearingPrice)
	}
	if !result.MatchedVolume.IsPositive() {
		t.Fatalf("matched volume = %s, want > 0", result.MatchedVolume)
	}
}

func TestRun_SelfMatchPrevention(t *testing.T) {
	bid := newOrder(utils.Buy, 1800, 10)
	ask := newOrder(utils.Sell, 1790, 10)
	ask.CommitmentKey = bid.CommitmentKey

	result := Run("ETH/USDC", []model.Order{bid}, []model.Order{ask})
	if result != nil {
		t.Fatal("expected nil result for self-match")
	}
}

func TestRun_EmptySide(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}

	result := Run("ETH/USDC", bids, nil)
	if result != nil {
		t.Fatal("expected nil for empty asks")
	}

	result = Run("ETH/USDC", nil, []model.Order{newOrder(utils.Sell, 1790, 10)})
	if result != nil {
		t.Fatal("expected nil for empty bids")
	}
}

func TestRun_ClearingPriceMaximizesVolume(t *testing.T) {
	bids := []model.Order{
		newOrder(utils.Buy, 110, 10),
		newOrder(utils.Buy, 100, 20),
	}
	asks := []model.Order{
		newOrder(utils.Sell, 90, 10),
		newOrder(utils.Sell, 100, 10),
		newOrder(utils.Sell, 110, 10),
	}

	result := Run("TEST/USD", bids, asks)
	if result == nil {
		t.Fatal("expected result")
	}
	if !result.ClearingPrice.Equal(decimal.NewFromInt(100)) {
		t.Fatalf("clearing price = %s, want 100", result.ClearingPrice)
	}
	if !result.MatchedVolume.Equal(decimal.NewFromInt(20)) {
		t.Fatalf("matched volume = %s, want 20", result.MatchedVolume)
	}
}
