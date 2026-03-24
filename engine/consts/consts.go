package consts

type Side int

const (
	Buy Side = iota
	Sell
)

func (s Side) String() string {
	if s == Buy {
		return "BUY"
	}
	return "SELL"
}

type EventType int

const (
	OrderPlacedType EventType = iota + 1
	OrderCancelledType
	OrderExpiredType
	AuctionExecutedType
	OrderMatchedType
	BatchSubmittedType
	BatchConfirmedType
)
