package event

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"encoding/gob"
	"errors"
	"fmt"
	"io"
	"os"
	"sync"
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
)

func init() {
	gob.Register(OrderPlaced{})
	gob.Register(OrderCancelled{})
	gob.Register(OrderExpired{})
	gob.Register(AuctionExecuted{})
	gob.Register(OrderMatched{})
	gob.Register(BatchSubmitted{})
	gob.Register(BatchConfirmed{})
}

const maxRecordBytes = 16 * 1024 * 1024

type FileStore struct {
	mu     sync.RWMutex
	path   string
	f      *os.File
	bw     *bufio.Writer
	events []Event
	seq    uint64
}

func OpenFileStore(path string) (*FileStore, error) {
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return nil, err
	}

	fs := &FileStore{path: path, f: f}
	if err := fs.loadAndTruncate(); err != nil {
		f.Close()
		return nil, err
	}

	if _, err := f.Seek(0, io.SeekEnd); err != nil {
		f.Close()
		return nil, err
	}
	fs.bw = bufio.NewWriter(f)
	return fs, nil
}

func (s *FileStore) loadAndTruncate() error {
	if _, err := s.f.Seek(0, io.SeekStart); err != nil {
		return err
	}

	var goodEnd int64
	br := bufio.NewReader(s.f)

	for {
		var length uint32
		if err := binary.Read(br, binary.BigEndian, &length); err != nil {
			if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
				break
			}
			return fmt.Errorf("read length at offset %d: %w", goodEnd, err)
		}

		if length == 0 || length > maxRecordBytes {
			break
		}

		buf := make([]byte, length)
		if _, err := io.ReadFull(br, buf); err != nil {
			if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
				break
			}
			return fmt.Errorf("read payload at offset %d: %w", goodEnd, err)
		}

		var evt Event
		if err := gob.NewDecoder(bytes.NewReader(buf)).Decode(&evt); err != nil {
			// Intentional fail-loud: partial-tail truncation handles records
			// where the length header or payload was never fully written.
			// A length that reads cleanly but decodes to garbage means
			// mid-record corruption (disk rot, wrong file, code regression),
			// which should block startup rather than silently discard data.
			return fmt.Errorf("corrupt event at offset %d: %w", goodEnd, err)
		}

		s.events = append(s.events, evt)
		if evt.Seq > s.seq {
			s.seq = evt.Seq
		}
		goodEnd += 4 + int64(length)
	}

	if err := s.f.Truncate(goodEnd); err != nil {
		return err
	}
	if _, err := s.f.Seek(goodEnd, io.SeekStart); err != nil {
		return err
	}
	return nil
}

func (s *FileStore) Append(events ...*Event) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	for _, e := range events {
		s.seq++
		e.Seq = s.seq
		if e.Timestamp.IsZero() {
			e.Timestamp = time.Now()
		}
	}

	var payload bytes.Buffer
	for _, e := range events {
		payload.Reset()
		if err := gob.NewEncoder(&payload).Encode(*e); err != nil {
			return err
		}
		length := uint32(payload.Len())
		if err := binary.Write(s.bw, binary.BigEndian, length); err != nil {
			return err
		}
		if _, err := s.bw.Write(payload.Bytes()); err != nil {
			return err
		}
	}

	if err := s.bw.Flush(); err != nil {
		return err
	}
	// fsync per Append: Append returns only after the batch is on stable
	// storage, so a crash can never confirm an order we later lose. Trades
	// throughput for durability; if this becomes the bottleneck, introduce a
	// group-commit path instead of dropping the sync. Note: s.mu is held
	// across Sync, so fsync latency stalls every concurrent Append and
	// ReadFrom. Acceptable at current scale; split into a write-mu / read-mu
	// pair if readers start blocking on writer fsyncs.
	if err := s.f.Sync(); err != nil {
		return err
	}

	for _, e := range events {
		s.events = append(s.events, *e)
	}
	return nil
}

func (s *FileStore) ReadFrom(afterSeq uint64, limit int) ([]Event, error) {
	if limit <= 0 {
		return nil, utils.ErrLimitMustBePositive
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	// Invariant: Append issues contiguous seqs starting at 1, so events[i]
	// has Seq == i+1. That lets us use afterSeq as the slice offset directly.
	start := int(afterSeq)
	if start > len(s.events) {
		start = len(s.events)
	}
	end := start + limit
	if end > len(s.events) {
		end = len(s.events)
	}

	out := make([]Event, end-start)
	copy(out, s.events[start:end])
	return out, nil
}

func (s *FileStore) LastSeq() uint64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.seq
}

func (s *FileStore) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.bw != nil {
		if err := s.bw.Flush(); err != nil {
			return err
		}
	}
	if s.f == nil {
		return nil
	}
	if err := s.f.Sync(); err != nil {
		return err
	}
	err := s.f.Close()
	s.f = nil
	return err
}
