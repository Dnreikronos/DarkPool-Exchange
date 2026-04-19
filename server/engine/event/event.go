package event

import (
	"time"

	"github.com/darkpool-exchange/server/engine/model"
	"github.com/darkpool-exchange/server/engine/utils"
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

// OrderPlaced holds the commitment, proof, and ciphertext. No plaintext fields;
// Recover decrypts in memory. SubmittedAt comes from Event.Timestamp and
// ExpiresAt is SubmittedAt + the TTL inside the ciphertext.
type OrderPlaced struct {
	OrderID    uuid.UUID
	Commitment []byte
	Proof      []byte
	Ciphertext []byte
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
	// Proof is the aggregated ZK proof produced before the on-chain submit. It
	// is persisted so crash-recovery can re-invoke Submit with the same proof
	// bytes instead of re-running the aggregator. nil when NoopAggregator is wired.
	Proof []byte
}

func (BatchSubmitted) eventData() {}

type BatchConfirmed struct {
	BatchID uuid.UUID
	TxHash  string
}

func (BatchConfirmed) eventData() {}

// BatchSettled records on-chain finality for a submitted batch: the tx that
// carried the submitBatch call has landed in a block. BatchConfirmed signals
// "RPC accepted the tx"; BatchSettled signals "chain accepted the tx".
type BatchSettled struct {
	BatchID     uuid.UUID
	BlockNumber uint64
	TxHash      string
}

func (BatchSettled) eventData() {}

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
