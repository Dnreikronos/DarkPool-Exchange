package engine

import (
	"testing"

	"github.com/darkpool-exchange/engine/utils"
	"github.com/darkpool-exchange/engine/model"
	"github.com/shopspring/decimal"
)

func TestRunAuction_BasicMatch(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1790, 10)}

	result := RunAuction("ETH/USDC", bids, asks)
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

func TestRunAuction_NoCrossing(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1700, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1800, 10)}

	result := RunAuction("ETH/USDC", bids, asks)
	if result != nil {
		t.Fatal("expected nil result when no crossing")
	}
}

func TestRunAuction_PartialFill(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}
	asks := []model.Order{newOrder(utils.Sell, 1790, 4)}

	result := RunAuction("ETH/USDC", bids, asks)
	if result == nil {
		t.Fatal("expected result")
	}
	if !result.MatchedVolume.Equal(decimal.NewFromInt(4)) {
		t.Fatalf("volume = %s, want 4", result.MatchedVolume)
	}
}

func TestRunAuction_MultipleBidsAndAsks(t *testing.T) {
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

	result := RunAuction("ETH/USDC", bids, asks)
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

func TestRunAuction_SelfMatchPrevention(t *testing.T) {
	bid := newOrder(utils.Buy, 1800, 10)
	ask := newOrder(utils.Sell, 1790, 10)
	ask.CommitmentKey = bid.CommitmentKey

	result := RunAuction("ETH/USDC", []model.Order{bid}, []model.Order{ask})
	if result != nil {
		t.Fatal("expected nil result for self-match")
	}
}

func TestRunAuction_EmptySide(t *testing.T) {
	bids := []model.Order{newOrder(utils.Buy, 1800, 10)}

	result := RunAuction("ETH/USDC", bids, nil)
	if result != nil {
		t.Fatal("expected nil for empty asks")
	}

	result = RunAuction("ETH/USDC", nil, []model.Order{newOrder(utils.Sell, 1790, 10)})
	if result != nil {
		t.Fatal("expected nil for empty bids")
	}
}

func TestRunAuction_ClearingPriceMaximizesVolume(t *testing.T) {
	bids := []model.Order{
		newOrder(utils.Buy, 110, 10),
		newOrder(utils.Buy, 100, 20),
	}
	asks := []model.Order{
		newOrder(utils.Sell, 90, 10),
		newOrder(utils.Sell, 100, 10),
		newOrder(utils.Sell, 110, 10),
	}

	result := RunAuction("TEST/USD", bids, asks)
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
