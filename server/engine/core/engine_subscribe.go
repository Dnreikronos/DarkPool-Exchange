package core

import (
	"time"

	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type AuctionNotification struct {
	AuctionID     uuid.UUID
	Pair          string
	ClearingPrice decimal.Decimal
	MatchedVolume decimal.Decimal
	MatchCount    int
	Timestamp     time.Time
}

type Subscriber struct {
	ID string
	Ch chan AuctionNotification
}

func (e *Engine) Subscribe(bufSize int) *Subscriber {
	if bufSize <= 0 {
		bufSize = 16
	}
	sub := &Subscriber{
		ID: uuid.New().String(),
		Ch: make(chan AuctionNotification, bufSize),
	}
	e.subMu.Lock()
	e.subscribers[sub.ID] = sub
	e.subMu.Unlock()
	return sub
}

func (e *Engine) Unsubscribe(id string) {
	e.subMu.Lock()
	if sub, ok := e.subscribers[id]; ok {
		close(sub.Ch)
		delete(e.subscribers, id)
	}
	e.subMu.Unlock()
}

func (e *Engine) notifySubscribers(n AuctionNotification) {
	e.subMu.RLock()
	defer e.subMu.RUnlock()

	for _, sub := range e.subscribers {
		select {
		case sub.Ch <- n:
		default:
			// subscriber too slow, drop notification
		}
	}
}
