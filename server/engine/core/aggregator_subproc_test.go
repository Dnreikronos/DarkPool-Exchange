package core

import (
	"context"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/darkpool-exchange/server/engine/event"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

func skipIfNoSh(t *testing.T) {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell fixtures not supported on windows")
	}
}

func abs(t *testing.T, rel string) string {
	t.Helper()
	p, err := filepath.Abs(rel)
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func sampleMatches() []event.OrderMatched {
	return []event.OrderMatched{{
		AuctionID: uuid.New(),
		Bid:       model.Fill{OrderID: uuid.New(), Size: decimal.NewFromInt(1)},
		Ask:       model.Fill{OrderID: uuid.New(), Size: decimal.NewFromInt(1)},
		Price:     decimal.NewFromInt(1800),
		Size:      decimal.NewFromInt(1),
	}}
}

func TestSubprocessAggregator_HappyPath(t *testing.T) {
	skipIfNoSh(t)
	a, err := NewSubprocessAggregator(abs(t, "testdata/fake_aggregator.sh"), 5*time.Second)
	if err != nil {
		t.Fatalf("new: %v", err)
	}
	proof, err := a.Aggregate(context.Background(), uuid.New(), sampleMatches())
	if err != nil {
		t.Fatalf("aggregate: %v", err)
	}
	if !strings.HasPrefix(string(proof), "fake-proof-") {
		t.Errorf("unexpected proof: %q", proof)
	}
}

func TestSubprocessAggregator_NonZeroExit(t *testing.T) {
	skipIfNoSh(t)
	a, err := NewSubprocessAggregator(abs(t, "testdata/fake_aggregator_fail.sh"), 5*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	_, err = a.Aggregate(context.Background(), uuid.New(), sampleMatches())
	if err == nil {
		t.Fatal("want error on non-zero exit")
	}
	if !strings.Contains(err.Error(), "simulated failure") {
		t.Errorf("stderr not surfaced: %v", err)
	}
}

func TestSubprocessAggregator_Timeout(t *testing.T) {
	skipIfNoSh(t)
	a, err := NewSubprocessAggregator(abs(t, "testdata/fake_aggregator_slow.sh"), 100*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	_, err = a.Aggregate(context.Background(), uuid.New(), sampleMatches())
	if err == nil {
		t.Fatal("want timeout error")
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Errorf("timeout not honored: waited %v", elapsed)
	}
}

func TestNewSubprocessAggregator_MissingBin(t *testing.T) {
	if _, err := NewSubprocessAggregator("/nonexistent/path/bin", time.Second); err == nil {
		t.Fatal("want error on missing bin")
	}
}
