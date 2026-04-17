package model

import (
	"time"

	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type Order struct {
	ID               uuid.UUID
	Pair             string
	Side             utils.Side
	Price            decimal.Decimal
	Size             decimal.Decimal
	RemainingSize    decimal.Decimal
	CommitmentKey    string
	EncryptedPayload []byte // opaque blob encrypted to operator pubkey; nil until the client-side encryption path lands
	SubmittedAt      time.Time
	ExpiresAt        time.Time
}

type Fill struct {
	OrderID uuid.UUID
	Size    decimal.Decimal
}
