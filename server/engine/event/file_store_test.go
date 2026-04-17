package event

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

func tempStorePath(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	return filepath.Join(dir, "events.log")
}

func TestFileStore_RoundTrip(t *testing.T) {
	path := tempStorePath(t)

	s, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}
	t.Cleanup(func() { s.Close() })

	events := []*Event{
		{Type: utils.OrderPlacedType, Data: OrderPlaced{}},
		{Type: utils.OrderCancelledType, Data: OrderCancelled{OrderID: uuid.New(), Reason: "test"}},
		{Type: utils.AuctionExecutedType, Data: AuctionExecuted{
			AuctionID:     uuid.New(),
			Pair:          "ETH/USDC",
			ClearingPrice: decimal.NewFromInt(1850),
			MatchedVolume: decimal.NewFromInt(10),
			MatchCount:    2,
		}},
	}

	if err := s.Append(events...); err != nil {
		t.Fatalf("Append: %v", err)
	}

	if got := s.LastSeq(); got != 3 {
		t.Fatalf("LastSeq = %d, want 3", got)
	}

	all, err := s.ReadFrom(0, 100)
	if err != nil {
		t.Fatalf("ReadFrom: %v", err)
	}
	if len(all) != 3 {
		t.Fatalf("ReadFrom len = %d, want 3", len(all))
	}
	for i, e := range all {
		if e.Seq != uint64(i+1) {
			t.Errorf("event[%d].Seq = %d, want %d", i, e.Seq, i+1)
		}
	}
}

func TestFileStore_PersistAcrossReopen(t *testing.T) {
	path := tempStorePath(t)

	s, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}

	cancelID := uuid.New()
	if err := s.Append(
		&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}},
		&Event{Type: utils.OrderCancelledType, Data: OrderCancelled{OrderID: cancelID, Reason: "r"}},
	); err != nil {
		t.Fatalf("Append: %v", err)
	}

	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	s2, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("Reopen: %v", err)
	}
	t.Cleanup(func() { s2.Close() })

	if got := s2.LastSeq(); got != 2 {
		t.Fatalf("reopened LastSeq = %d, want 2", got)
	}

	all, err := s2.ReadFrom(0, 100)
	if err != nil {
		t.Fatalf("ReadFrom after reopen: %v", err)
	}
	if len(all) != 2 {
		t.Fatalf("reopen len = %d, want 2", len(all))
	}

	oc, ok := all[1].Data.(OrderCancelled)
	if !ok {
		t.Fatalf("all[1].Data = %T, want OrderCancelled", all[1].Data)
	}
	if oc.OrderID != cancelID {
		t.Errorf("OrderID = %s, want %s", oc.OrderID, cancelID)
	}

	if err := s2.Append(&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}}); err != nil {
		t.Fatalf("Append after reopen: %v", err)
	}
	if got := s2.LastSeq(); got != 3 {
		t.Fatalf("LastSeq after append = %d, want 3", got)
	}
}

func TestFileStore_TruncatesPartialTail(t *testing.T) {
	path := tempStorePath(t)

	s, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}
	if err := s.Append(
		&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}},
		&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}},
	); err != nil {
		t.Fatalf("Append: %v", err)
	}
	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	f, err := os.OpenFile(path, os.O_RDWR|os.O_APPEND, 0o644)
	if err != nil {
		t.Fatalf("reopen for corrupt: %v", err)
	}
	if _, err := f.Write([]byte{0x00, 0x00}); err != nil {
		t.Fatalf("write partial: %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close corrupt: %v", err)
	}

	s2, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("reopen after corrupt: %v", err)
	}
	t.Cleanup(func() { s2.Close() })

	if got := s2.LastSeq(); got != 2 {
		t.Fatalf("after truncate LastSeq = %d, want 2", got)
	}

	if err := s2.Append(&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}}); err != nil {
		t.Fatalf("Append after truncate: %v", err)
	}
	if got := s2.LastSeq(); got != 3 {
		t.Fatalf("LastSeq after post-truncate append = %d, want 3", got)
	}

	if err := s2.Close(); err != nil {
		t.Fatalf("close s2: %v", err)
	}
	s3, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("third open: %v", err)
	}
	t.Cleanup(func() { s3.Close() })
	if got := s3.LastSeq(); got != 3 {
		t.Fatalf("third open LastSeq = %d, want 3", got)
	}
}

func TestFileStore_TruncatesOversizeLength(t *testing.T) {
	path := tempStorePath(t)

	s, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}
	if err := s.Append(&Event{Type: utils.OrderPlacedType, Data: OrderPlaced{}}); err != nil {
		t.Fatalf("Append: %v", err)
	}
	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	// bogus length header 0xFFFFFFFF would allocate 4GB without the cap
	f, err := os.OpenFile(path, os.O_RDWR|os.O_APPEND, 0o644)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	if _, err := f.Write([]byte{0xFF, 0xFF, 0xFF, 0xFF}); err != nil {
		t.Fatalf("write bad length: %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	s2, err := OpenFileStore(path)
	if err != nil {
		t.Fatalf("reopen after corrupt: %v", err)
	}
	t.Cleanup(func() { s2.Close() })

	if got := s2.LastSeq(); got != 1 {
		t.Fatalf("LastSeq = %d, want 1 (oversize tail should be truncated)", got)
	}
}

func TestFileStore_InvalidLimit(t *testing.T) {
	s, err := OpenFileStore(tempStorePath(t))
	if err != nil {
		t.Fatalf("OpenFileStore: %v", err)
	}
	t.Cleanup(func() { s.Close() })

	if _, err := s.ReadFrom(0, 0); err == nil {
		t.Fatal("expected error for limit=0")
	}
}
