package handler

import (
	"context"
	"errors"
	"time"

	darkpoolv1 "github.com/darkpool-exchange/server/api/gen/darkpool/v1"
	apiutils "github.com/darkpool-exchange/server/api/utils"
	"github.com/darkpool-exchange/server/engine/core"
	"github.com/darkpool-exchange/server/engine/model"
	"github.com/darkpool-exchange/server/engine/utils"
	"github.com/google/uuid"
	"github.com/shopspring/decimal"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

type Server struct {
	darkpoolv1.UnimplementedDarkPoolServiceServer
	engine *core.Engine
}

func NewServer(eng *core.Engine) *Server {
	return &Server{engine: eng}
}

func (s *Server) PlaceOrder(ctx context.Context, req *darkpoolv1.PlaceOrderRequest) (*darkpoolv1.PlaceOrderResponse, error) {
	price, err := decimal.NewFromString(req.Price)
	if err != nil {
		return nil, status.Errorf(codes.InvalidArgument, "invalid price: %v", err)
	}
	size, err := decimal.NewFromString(req.Size)
	if err != nil {
		return nil, status.Errorf(codes.InvalidArgument, "invalid size: %v", err)
	}

	var side utils.Side
	switch req.Side {
	case darkpoolv1.Side_SIDE_BUY:
		side = utils.Buy
	case darkpoolv1.Side_SIDE_SELL:
		side = utils.Sell
	default:
		return nil, status.Error(codes.InvalidArgument, apiutils.MsgInvalidSide)
	}

	ttl := time.Duration(req.TtlSeconds) * time.Second

	order, err := s.engine.PlaceOrder(req.Pair, side, price, size, req.CommitmentKey, ttl)
	if err != nil {
		if errors.Is(err, utils.ErrPairRequired) ||
			errors.Is(err, utils.ErrPriceMustBePositive) ||
			errors.Is(err, utils.ErrSizeMustBePositive) ||
			errors.Is(err, utils.ErrCommitmentKeyRequired) {
			return nil, status.Errorf(codes.InvalidArgument, "%v", err)
		}
		return nil, status.Errorf(codes.Internal, "failed to place order: %v", err)
	}

	return &darkpoolv1.PlaceOrderResponse{
		Order: modelOrderToProto(order),
	}, nil
}

func (s *Server) CancelOrder(ctx context.Context, req *darkpoolv1.CancelOrderRequest) (*darkpoolv1.CancelOrderResponse, error) {
	id, err := uuid.Parse(req.OrderId)
	if err != nil {
		return nil, status.Errorf(codes.InvalidArgument, "invalid order_id: %v", err)
	}

	if err := s.engine.CancelOrder(id, req.Reason); err != nil {
		if errors.Is(err, utils.ErrOrderNotFound) {
			return nil, status.Errorf(codes.NotFound, "%v", err)
		}
		return nil, status.Errorf(codes.Internal, "failed to cancel order: %v", err)
	}

	return &darkpoolv1.CancelOrderResponse{}, nil
}

func (s *Server) GetOrder(ctx context.Context, req *darkpoolv1.GetOrderRequest) (*darkpoolv1.GetOrderResponse, error) {
	id, err := uuid.Parse(req.OrderId)
	if err != nil {
		return nil, status.Errorf(codes.InvalidArgument, "invalid order_id: %v", err)
	}

	order := s.engine.GetOrder(id)
	if order == nil {
		return nil, status.Errorf(codes.NotFound, "order %s not found", req.OrderId)
	}

	return &darkpoolv1.GetOrderResponse{
		Order: modelOrderToProto(order),
	}, nil
}

func (s *Server) GetOrderBook(ctx context.Context, req *darkpoolv1.GetOrderBookRequest) (*darkpoolv1.GetOrderBookResponse, error) {
	if req.Pair == "" {
		return nil, status.Error(codes.InvalidArgument, apiutils.MsgPairRequired)
	}

	bids, asks := s.engine.GetOrderBook(req.Pair)

	return &darkpoolv1.GetOrderBookResponse{
		Pair: req.Pair,
		Bids: aggregateLevels(bids),
		Asks: aggregateLevels(asks),
	}, nil
}

func (s *Server) GetAuctionHistory(ctx context.Context, req *darkpoolv1.GetAuctionHistoryRequest) (*darkpoolv1.GetAuctionHistoryResponse, error) {
	history, err := s.engine.GetAuctionHistory(req.Pair, int(req.Limit))
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to read auction history: %v", err)
	}

	summaries := make([]*darkpoolv1.AuctionSummary, len(history))
	for i, ae := range history {
		summaries[i] = &darkpoolv1.AuctionSummary{
			AuctionId:     ae.AuctionID.String(),
			Pair:          ae.Pair,
			ClearingPrice: ae.ClearingPrice.String(),
			MatchedVolume: ae.MatchedVolume.String(),
			MatchCount:    int32(ae.MatchCount),
			TimestampUnix: ae.Timestamp.Unix(),
		}
	}

	return &darkpoolv1.GetAuctionHistoryResponse{Auctions: summaries}, nil
}

func (s *Server) StreamAuctions(req *darkpoolv1.StreamAuctionsRequest, stream darkpoolv1.DarkPoolService_StreamAuctionsServer) error {
	sub := s.engine.Subscribe(32)
	defer s.engine.Unsubscribe(sub.ID)

	for {
		select {
		case <-stream.Context().Done():
			return nil
		case n, ok := <-sub.Ch:
			if !ok {
				return nil
			}
			if req.Pair != "" && n.Pair != req.Pair {
				continue
			}
			if err := stream.Send(&darkpoolv1.AuctionEvent{
				AuctionId:     n.AuctionID.String(),
				Pair:          n.Pair,
				ClearingPrice: n.ClearingPrice.String(),
				MatchedVolume: n.MatchedVolume.String(),
				MatchCount:    int32(n.MatchCount),
				TimestampUnix: n.Timestamp.Unix(),
			}); err != nil {
				return err
			}
		}
	}
}

func modelOrderToProto(o *model.Order) *darkpoolv1.OrderInfo {
	if o == nil {
		return nil
	}
	var side darkpoolv1.Side
	switch o.Side {
	case utils.Buy:
		side = darkpoolv1.Side_SIDE_BUY
	case utils.Sell:
		side = darkpoolv1.Side_SIDE_SELL
	}
	return &darkpoolv1.OrderInfo{
		Id:              o.ID.String(),
		Pair:            o.Pair,
		Side:            side,
		Price:           o.Price.String(),
		Size:            o.Size.String(),
		RemainingSize:   o.RemainingSize.String(),
		CommitmentKey:   o.CommitmentKey,
		SubmittedAtUnix: o.SubmittedAt.Unix(),
		ExpiresAtUnix:   o.ExpiresAt.Unix(),
	}
}

func aggregateLevels(orders []model.Order) []*darkpoolv1.PriceLevel {
	type level struct {
		total decimal.Decimal
		count int
	}
	agg := make(map[string]*level)
	var priceOrder []string

	for _, o := range orders {
		key := o.Price.String()
		if _, ok := agg[key]; !ok {
			agg[key] = &level{}
			priceOrder = append(priceOrder, key)
		}
		agg[key].total = agg[key].total.Add(o.RemainingSize)
		agg[key].count++
	}

	out := make([]*darkpoolv1.PriceLevel, 0, len(agg))
	for _, key := range priceOrder {
		l := agg[key]
		out = append(out, &darkpoolv1.PriceLevel{
			Price:      key,
			TotalSize:  l.total.String(),
			OrderCount: int32(l.count),
		})
	}
	return out
}
