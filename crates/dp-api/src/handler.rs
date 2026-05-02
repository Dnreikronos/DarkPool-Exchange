use std::pin::Pin;

use dp_engine::Engine;
use futures_util::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::conv::{
    aggregate_levels, notification_to_event, notification_to_summary, order_to_proto, parse_uuid,
};
use crate::error::engine_error_to_status;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    AuctionEvent, CancelOrderRequest, CancelOrderResponse, GetAuctionHistoryRequest,
    GetAuctionHistoryResponse, GetOrderBookRequest, GetOrderBookResponse, GetOrderRequest,
    GetOrderResponse, PlaceOrderRequest, PlaceOrderResponse, StreamAuctionsRequest,
};
use crate::validation::{validate_place_order, MSG_PAIR_REQUIRED};

#[derive(Clone)]
pub struct ApiHandler {
    pub engine: Engine,
}

impl ApiHandler {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }
}

#[tonic::async_trait]
impl DarkPoolService for ApiHandler {
    type StreamAuctionsStream =
        Pin<Box<dyn Stream<Item = Result<AuctionEvent, Status>> + Send + 'static>>;

    async fn place_order(
        &self,
        req: Request<PlaceOrderRequest>,
    ) -> Result<Response<PlaceOrderResponse>, Status> {
        let req = req.into_inner();
        validate_place_order(&req)?;
        let order = self
            .engine
            .place_encrypted_order(req.commitment, req.proof, req.encrypted_payload)
            .await
            .map_err(engine_error_to_status)?;
        Ok(Response::new(PlaceOrderResponse {
            order: Some(order_to_proto(&order)),
        }))
    }

    async fn cancel_order(
        &self,
        req: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        let req = req.into_inner();
        let id = parse_uuid(&req.order_id)?;
        let reason = if req.reason.is_empty() {
            None
        } else {
            Some(req.reason)
        };
        self.engine
            .cancel_order(id, reason)
            .map_err(engine_error_to_status)?;
        Ok(Response::new(CancelOrderResponse {}))
    }

    async fn get_order(
        &self,
        req: Request<GetOrderRequest>,
    ) -> Result<Response<GetOrderResponse>, Status> {
        let req = req.into_inner();
        let id = parse_uuid(&req.order_id)?;
        match self.engine.get_order(id) {
            Some(o) => Ok(Response::new(GetOrderResponse {
                order: Some(order_to_proto(&o)),
            })),
            None => Err(Status::not_found(format!("order {} not found", req.order_id))),
        }
    }

    async fn get_order_book(
        &self,
        req: Request<GetOrderBookRequest>,
    ) -> Result<Response<GetOrderBookResponse>, Status> {
        let req = req.into_inner();
        if req.pair.is_empty() {
            return Err(Status::invalid_argument(MSG_PAIR_REQUIRED));
        }
        let (bids, asks) = self.engine.get_order_book(&req.pair);
        Ok(Response::new(GetOrderBookResponse {
            pair: req.pair,
            bids: aggregate_levels(bids),
            asks: aggregate_levels(asks),
        }))
    }

    async fn get_auction_history(
        &self,
        req: Request<GetAuctionHistoryRequest>,
    ) -> Result<Response<GetAuctionHistoryResponse>, Status> {
        let req = req.into_inner();
        let pair = if req.pair.is_empty() {
            None
        } else {
            Some(req.pair.as_str())
        };
        let limit = req.limit.max(0) as usize;
        let history = self
            .engine
            .get_auction_history(pair, limit)
            .map_err(engine_error_to_status)?;
        let auctions = history.iter().map(notification_to_summary).collect();
        Ok(Response::new(GetAuctionHistoryResponse { auctions }))
    }

    async fn stream_auctions(
        &self,
        req: Request<StreamAuctionsRequest>,
    ) -> Result<Response<Self::StreamAuctionsStream>, Status> {
        let pair = req.into_inner().pair;
        let rx = self.engine.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(move |item| match item {
            Ok(n) => {
                if !pair.is_empty() && n.pair != pair {
                    None
                } else {
                    Some(Ok(notification_to_event(&n)))
                }
            }
            Err(_lag) => None,
        });
        Ok(Response::new(Box::pin(stream)))
    }
}
