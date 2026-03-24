package model

import (
	"time"

	"github.com/darkpool-exchange/engine/consts"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type Order struct {
	ID            uuid.UUID
	Pair          string
	Side          consts.Side
	Price         decimal.Decimal
	Size          decimal.Decimal
	RemainingSize decimal.Decimal
	CommitmentKey string
	SubmittedAt   time.Time
	ExpiresAt     time.Time
}

type Fill struct {
	OrderID uuid.UUID
	Size    decimal.Decimal
}
