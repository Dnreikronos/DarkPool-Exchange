package event

import (
	"time"

	"github.com/darkpool-exchange/engine/consts"
	"github.com/darkpool-exchange/engine/model"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type Event struct {
	Seq       uint64
	Type      consts.EventType
	Timestamp time.Time
	Data      any
}

type OrderPlaced struct {
	Order model.Order
}

type OrderCancelled struct {
	OrderID uuid.UUID
	Reason  string
}

type OrderExpired struct {
	OrderID uuid.UUID
}

type AuctionExecuted struct {
	AuctionID     uuid.UUID
	Pair          string
	ClearingPrice decimal.Decimal
	MatchedVolume decimal.Decimal
	Timestamp     time.Time
}

type OrderMatched struct {
	AuctionID uuid.UUID
	Bid       model.Fill
	Ask       model.Fill
	Price     decimal.Decimal
	Size      decimal.Decimal
}

type BatchSubmitted struct {
	BatchID   uuid.UUID
	AuctionID uuid.UUID
	TxHash    string
	PairCount int
}

type BatchConfirmed struct {
	BatchID uuid.UUID
	TxHash  string
}

// Store is the interface for the append-only event log.
type Store interface {
	Append(events ...Event) error
	ReadFrom(afterSeq uint64, limit int) ([]Event, error)
	LastSeq() uint64
}
