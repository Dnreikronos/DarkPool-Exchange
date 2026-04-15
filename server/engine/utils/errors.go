package utils

import "errors"

var (
	ErrPairRequired          = errors.New("pair is required")
	ErrPriceMustBePositive   = errors.New("price must be positive")
	ErrSizeMustBePositive    = errors.New("size must be positive")
	ErrCommitmentKeyRequired = errors.New("commitment key is required")
	ErrLimitMustBePositive   = errors.New("limit must be > 0")
	ErrOrderNotFound         = errors.New("order not found")
)
