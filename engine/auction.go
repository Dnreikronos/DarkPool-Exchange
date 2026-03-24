package engine

import (
	"sort"

	"github.com/darkpool-exchange/engine/event"
	"github.com/darkpool-exchange/engine/model"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
)

type AuctionResult struct {
	AuctionID     uuid.UUID
	Pair          string
	ClearingPrice decimal.Decimal
	MatchedVolume decimal.Decimal
	Matches       []event.OrderMatched
}

// RunAuction executes a batch auction for a single trading pair.
//
// Algorithm:
//  1. Collect all candidate prices from bids and asks.
//  2. For each candidate, compute matched volume as
//     min(cumulative bid volume at >= price, cumulative ask volume at <= price).
//  3. Pick the candidate that maximizes matched volume.
//  4. Match individual orders at the clearing price with partial fills
//     and self-match prevention.
func RunAuction(pair string, bids, asks []model.Order) *AuctionResult {
	if len(bids) == 0 || len(asks) == 0 {
		return nil
	}

	sort.Slice(bids, func(i, j int) bool { return bids[i].Price.GreaterThan(bids[j].Price) })
	sort.Slice(asks, func(i, j int) bool { return asks[i].Price.LessThan(asks[j].Price) })

	if bids[0].Price.LessThan(asks[0].Price) {
		return nil
	}

	clearingPrice := computeClearingPrice(bids, asks)
	if !clearingPrice.IsPositive() {
		return nil
	}

	matches := matchOrders(bids, asks, clearingPrice)
	if len(matches) == 0 {
		return nil
	}

	var totalVolume decimal.Decimal
	for _, m := range matches {
		totalVolume = totalVolume.Add(m.Size)
	}

	auctionID := uuid.New()
	for i := range matches {
		matches[i].AuctionID = auctionID
	}

	return &AuctionResult{
		AuctionID:     auctionID,
		Pair:          pair,
		ClearingPrice: clearingPrice,
		MatchedVolume: totalVolume,
		Matches:       matches,
	}
}

func computeClearingPrice(bids, asks []model.Order) decimal.Decimal {
	priceSet := make(map[string]decimal.Decimal)
	for _, b := range bids {
		priceSet[b.Price.String()] = b.Price
	}
	for _, a := range asks {
		priceSet[a.Price.String()] = a.Price
	}

	prices := make([]decimal.Decimal, 0, len(priceSet))
	for _, p := range priceSet {
		prices = append(prices, p)
	}
	sort.Slice(prices, func(i, j int) bool { return prices[i].LessThan(prices[j]) })

	bestVolume := decimal.Zero

	var tiedPrices []decimal.Decimal

	for _, p := range prices {
		bidVol := cumulativeVolume(bids, func(o *model.Order) bool { return o.Price.GreaterThanOrEqual(p) })
		askVol := cumulativeVolume(asks, func(o *model.Order) bool { return o.Price.LessThanOrEqual(p) })
		matched := decimal.Min(bidVol, askVol)

		if matched.GreaterThan(bestVolume) {
			bestVolume = matched
			tiedPrices = []decimal.Decimal{p}
		} else if matched.Equal(bestVolume) && matched.IsPositive() {
			tiedPrices = append(tiedPrices, p)
		}
	}

	if len(tiedPrices) == 0 {
		return decimal.Zero
	}

	// Tie-breaking: use the mid-price of the lowest and highest tied candidates.
	lo := tiedPrices[0]
	hi := tiedPrices[len(tiedPrices)-1]
	return lo.Add(hi).Div(decimal.NewFromInt(2))
}

func cumulativeVolume(orders []model.Order, pred func(*model.Order) bool) decimal.Decimal {
	var vol decimal.Decimal
	for i := range orders {
		if pred(&orders[i]) {
			vol = vol.Add(orders[i].RemainingSize)
		}
	}
	return vol
}

func matchOrders(bids, asks []model.Order, price decimal.Decimal) []event.OrderMatched {
	var eligibleBids, eligibleAsks []model.Order
	for _, b := range bids {
		if b.Price.GreaterThanOrEqual(price) && b.RemainingSize.IsPositive() {
			eligibleBids = append(eligibleBids, b)
		}
	}
	for _, a := range asks {
		if a.Price.LessThanOrEqual(price) && a.RemainingSize.IsPositive() {
			eligibleAsks = append(eligibleAsks, a)
		}
	}

	var matches []event.OrderMatched

	for bi := 0; bi < len(eligibleBids); bi++ {
		bid := &eligibleBids[bi]

		for ai := 0; ai < len(eligibleAsks) && bid.RemainingSize.IsPositive(); ai++ {
			ask := &eligibleAsks[ai]

			if bid.CommitmentKey == ask.CommitmentKey {
				continue
			}

			if !ask.RemainingSize.IsPositive() {
				continue
			}

			fillSize := decimal.Min(bid.RemainingSize, ask.RemainingSize)

			matches = append(matches, event.OrderMatched{
				Bid:   model.Fill{OrderID: bid.ID, Size: fillSize},
				Ask:   model.Fill{OrderID: ask.ID, Size: fillSize},
				Price: price,
				Size:  fillSize,
			})

			bid.RemainingSize = bid.RemainingSize.Sub(fillSize)
			ask.RemainingSize = ask.RemainingSize.Sub(fillSize)
		}
	}

	return matches
}
