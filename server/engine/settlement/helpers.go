package settlement

import (
	"math/big"

	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

// UUIDToBytes32 left-pads a UUID (16 bytes) into a 32-byte word. The high
// 16 bytes are zero, which the settlement contract treats as unused.
func UUIDToBytes32(id uuid.UUID) [32]byte {
	var out [32]byte
	copy(out[16:], id[:])
	return out
}

// decimalToWei scales a shopspring/decimal to a uint256-compatible integer
// using 18 decimals (standard ERC-20 precision). Returns ok=false if the
// value is negative or loses precision.
func decimalToWei(d decimal.Decimal) (*big.Int, bool) {
	if d.Sign() < 0 {
		return nil, false
	}
	scaled := d.Shift(18)
	if !scaled.Equal(scaled.Truncate(0)) {
		return nil, false
	}
	return scaled.BigInt(), true
}
