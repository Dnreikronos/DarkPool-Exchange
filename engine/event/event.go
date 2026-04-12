package event

import (
	"time"

	"github.com/darkpool-exchange/engine/utils"
	"github.com/darkpool-exchange/engine/model"
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
	BatchID   uuid.UUID
	AuctionID uuid.UUID
	TxHash    string
	PairCount int
}

func (BatchSubmitted) eventData() {}

type BatchConfirmed struct {
	BatchID uuid.UUID
	TxHash  string
}

func (BatchConfirmed) eventData() {}

// Store is the interface for the append-only event log.
type Store interface {
	Append(events ...Event) error
	ReadFrom(afterSeq uint64, limit int) ([]Event, error)
	LastSeq() uint64
}
