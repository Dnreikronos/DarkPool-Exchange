package event

import (
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type EventData interface {
	eventData()
}

type Event struct {
	Seq       uint64
	Type      utils.EventType
	Timestamp time.Time
	Data      EventData
}

type OrderPlaced struct {
	Order model.Order
}

func (OrderPlaced) eventData() {}

type OrderCancelled struct {
	OrderID uuid.UUID
	Reason  string
}

func (OrderCancelled) eventData() {}

type OrderExpired struct {
	OrderID uuid.UUID
}

func (OrderExpired) eventData() {}

type AuctionExecuted struct {
	AuctionID     uuid.UUID
	Pair          string
	ClearingPrice decimal.Decimal
	MatchedVolume decimal.Decimal
	MatchCount    int
	Timestamp     time.Time
}

func (AuctionExecuted) eventData() {}

type OrderMatched struct {
	AuctionID uuid.UUID
	Bid       model.Fill
	Ask       model.Fill
	Price     decimal.Decimal
	Size      decimal.Decimal
}

func (OrderMatched) eventData() {}

type BatchSubmitted struct {
	BatchID    uuid.UUID
	AuctionID  uuid.UUID
	TxHash     string
	MatchCount int
}

func (BatchSubmitted) eventData() {}

type BatchConfirmed struct {
	BatchID uuid.UUID
	TxHash  string
}

func (BatchConfirmed) eventData() {}

// Store is the append-only event log.
//
// Append assigns Seq in place on each *Event. Callers that Apply afterward
// must use the same *Event so projection.seq advances; passing by value
// would leave Seq=0 and silently break OrderBook.Replay.
type Store interface {
	Append(events ...*Event) error
	ReadFrom(afterSeq uint64, limit int) ([]Event, error)
	LastSeq() uint64
}
