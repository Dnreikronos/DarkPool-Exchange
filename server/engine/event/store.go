package event

import (
	"sync"
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
)

type MemStore struct {
	mu     sync.RWMutex
	events []Event
	seq    uint64
}

func NewMemStore() *MemStore {
	return &MemStore{}
}

func (s *MemStore) Append(events ...Event) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	buf := make([]Event, len(events))
	copy(buf, events)

	for i := range buf {
		s.seq++
		buf[i].Seq = s.seq
		if buf[i].Timestamp.IsZero() {
			buf[i].Timestamp = time.Now()
		}
	}
	s.events = append(s.events, buf...)
	return nil
}

func (s *MemStore) ReadFrom(afterSeq uint64, limit int) ([]Event, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if limit <= 0 {
		return nil, utils.ErrLimitMustBePositive
	}

	start := s.indexAfter(afterSeq)
	if start >= len(s.events) {
		return nil, nil
	}

	end := start + limit
	if end > len(s.events) {
		end = len(s.events)
	}

	out := make([]Event, end-start)
	copy(out, s.events[start:end])
	return out, nil
}

func (s *MemStore) LastSeq() uint64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.seq
}

// Sequences are contiguous starting at 1, so the index is simply afterSeq.
func (s *MemStore) indexAfter(afterSeq uint64) int {
	if afterSeq == 0 {
		return 0
	}
	idx := int(afterSeq)
	if idx > len(s.events) {
		return len(s.events)
	}
	return idx
}
