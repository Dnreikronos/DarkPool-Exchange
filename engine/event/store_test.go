package event

import (
	"testing"

	"github.com/darkpool-exchange/engine/consts"
	"github.com/google/uuid"
)

func TestMemStore_AppendAndRead(t *testing.T) {
	s := NewMemStore()

	events := []Event{
		{Type: consts.OrderPlacedType, Data: OrderPlaced{}},
		{Type: consts.OrderCancelledType, Data: OrderCancelled{OrderID: uuid.New()}},
		{Type: consts.OrderPlacedType, Data: OrderPlaced{}},
	}

	if err := s.Append(events...); err != nil {
		t.Fatalf("Append: %v", err)
	}

	if got := s.LastSeq(); got != 3 {
		t.Fatalf("LastSeq = %d, want 3", got)
	}

	// Read all from beginning.
	all, err := s.ReadFrom(0, 100)
	if err != nil {
		t.Fatalf("ReadFrom(0): %v", err)
	}
	if len(all) != 3 {
		t.Fatalf("ReadFrom(0) len = %d, want 3", len(all))
	}
	for i, e := range all {
		if e.Seq != uint64(i+1) {
			t.Errorf("event[%d].Seq = %d, want %d", i, e.Seq, i+1)
		}
	}

	// Read from middle.
	tail, err := s.ReadFrom(2, 100)
	if err != nil {
		t.Fatalf("ReadFrom(2): %v", err)
	}
	if len(tail) != 1 {
		t.Fatalf("ReadFrom(2) len = %d, want 1", len(tail))
	}
	if tail[0].Seq != 3 {
		t.Errorf("tail[0].Seq = %d, want 3", tail[0].Seq)
	}

	// Read past end.
	empty, err := s.ReadFrom(3, 100)
	if err != nil {
		t.Fatalf("ReadFrom(3): %v", err)
	}
	if len(empty) != 0 {
		t.Fatalf("ReadFrom(3) len = %d, want 0", len(empty))
	}
}

func TestMemStore_ReadFromInvalidLimit(t *testing.T) {
	s := NewMemStore()
	_, err := s.ReadFrom(0, 0)
	if err == nil {
		t.Fatal("expected error for limit=0")
	}
}

func TestMemStore_EmptyStore(t *testing.T) {
	s := NewMemStore()
	if got := s.LastSeq(); got != 0 {
		t.Fatalf("LastSeq on empty store = %d, want 0", got)
	}
	events, err := s.ReadFrom(0, 10)
	if err != nil {
		t.Fatalf("ReadFrom on empty store: %v", err)
	}
	if len(events) != 0 {
		t.Fatalf("ReadFrom on empty store len = %d, want 0", len(events))
	}
}
