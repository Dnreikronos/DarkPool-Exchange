use std::pin::Pin;

use dp_engine::AuctionNotification;
use dp_engine::Engine;
use futures_util::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::conv::{
    aggregate_levels, notification_to_event, notification_to_summary, order_to_proto,
    pair_config_to_proto, parse_uuid,
};
use crate::error::engine_error_to_status;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    AuctionEvent, CancelOrderRequest, CancelOrderResponse, GetAuctionHistoryRequest,
    GetAuctionHistoryResponse, GetOrderBookRequest, GetOrderBookResponse, GetOrderRequest,
    GetOrderResponse, ListPairsRequest, ListPairsResponse, PlaceOrderRequest, PlaceOrderResponse,
    StreamAuctionsRequest,
};
use crate::validation::{validate_pair_for_history, validate_pair_known, validate_place_order};

/// Map one BroadcastStream item to an outgoing auction event:
/// - Ok matches the pair filter → forward as event.
/// - Ok doesn't match the pair filter → drop.
/// - Err(Lagged(n)) → surface as a `data_loss` Status so the client knows
///   they missed events (we don't silently swallow lag).
fn map_broadcast_item(
    pair: &str,
    item: Result<AuctionNotification, tokio_stream::wrappers::errors::BroadcastStreamRecvError>,
) -> Option<Result<AuctionEvent, Status>> {
    match item {
        Ok(n) => {
            if !pair.is_empty() && n.pair != pair {
                None
            } else {
                Some(Ok(notification_to_event(&n)))
            }
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => Some(Err(
            Status::data_loss(format!("stream lagged; {} auction events skipped", n)),
        )),
    }
}

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
        let caller = req
            .extensions()
            .get::<crate::auth::AuthenticatedIdentity>()
            .and_then(|id| match id {
                crate::auth::AuthenticatedIdentity::Wallet(addr) => Some(*addr),
                crate::auth::AuthenticatedIdentity::ApiKey => None,
            });
        let req = req.into_inner();
        validate_place_order(&req)?;
        let order = self
            .engine
            .place_encrypted_order(req.commitment, req.proof, req.encrypted_payload, caller)
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
            None => Err(Status::not_found(format!(
                "order {} not found",
                req.order_id
            ))),
        }
    }

    async fn get_order_book(
        &self,
        req: Request<GetOrderBookRequest>,
    ) -> Result<Response<GetOrderBookResponse>, Status> {
        let req = req.into_inner();
        // Empty / whitespace-only `pair` is caught by `Pair::parse` inside
        // `validate_pair_known` (returns `PairRequired` → invalid_argument
        // with `MSG_PAIR_REQUIRED`); no separate guard needed here.
        let canonical = validate_pair_known(&self.engine, &req.pair)?;
        let (bids, asks) = self.engine.get_order_book(canonical.as_str());
        Ok(Response::new(GetOrderBookResponse {
            pair: canonical.into_string(),
            bids: aggregate_levels(bids),
            asks: aggregate_levels(asks),
        }))
    }

    async fn get_auction_history(
        &self,
        req: Request<GetAuctionHistoryRequest>,
    ) -> Result<Response<GetAuctionHistoryResponse>, Status> {
        let req = req.into_inner();
        // History accepts Delisted pairs: the auction log retains records
        // past a delist, and delisting is forward-looking (no new orders /
        // auctions) rather than retroactive.
        let canonical = if req.pair.is_empty() {
            None
        } else {
            Some(validate_pair_for_history(&self.engine, &req.pair)?)
        };
        // proto field is i32; clamp negatives to 0 (treated as "no limit" by engine).
        let limit = req.limit.max(0) as usize;
        let history = self
            .engine
            .get_auction_history(canonical.as_ref().map(|p| p.as_str()), limit)
            .map_err(engine_error_to_status)?;
        let auctions = history.iter().map(notification_to_summary).collect();
        Ok(Response::new(GetAuctionHistoryResponse { auctions }))
    }

    async fn stream_auctions(
        &self,
        req: Request<StreamAuctionsRequest>,
    ) -> Result<Response<Self::StreamAuctionsStream>, Status> {
        let raw = req.into_inner().pair;
        let pair = if raw.is_empty() {
            String::new()
        } else {
            validate_pair_known(&self.engine, &raw)?.into_string()
        };
        let rx = self.engine.subscribe();
        let stream =
            BroadcastStream::new(rx).filter_map(move |item| map_broadcast_item(&pair, item));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn list_pairs(
        &self,
        _req: Request<ListPairsRequest>,
    ) -> Result<Response<ListPairsResponse>, Status> {
        // Public surface: active pairs only.
        let pairs = self
            .engine
            .list_pairs()
            .into_iter()
            .filter(|(_, cfg)| cfg.status == dp_engine::PairStatus::Active)
            .map(|(p, c)| pair_config_to_proto(&p, &c))
            .collect();
        Ok(Response::new(ListPairsResponse { pairs }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
    use uuid::Uuid;

    fn notif(pair: &str) -> AuctionNotification {
        AuctionNotification {
            auction_id: Uuid::nil(),
            pair: pair.to_string(),
            clearing_price: Decimal::ONE,
            matched_volume: Decimal::ONE,
            match_count: 1,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn map_broadcast_item_lag_becomes_data_loss_status() {
        let out = map_broadcast_item("", Err(BroadcastStreamRecvError::Lagged(7)));
        let item = out.expect("lag produces some(Err)");
        let err = item.expect_err("lag is Err");
        assert_eq!(err.code(), tonic::Code::DataLoss);
        assert!(err.message().contains('7'));
    }

    #[test]
    fn map_broadcast_item_passes_through_when_pair_filter_empty() {
        let out = map_broadcast_item("", Ok(notif("ETH/USDC")));
        let item = out.expect("event passes through");
        assert!(item.is_ok());
    }

    #[test]
    fn map_broadcast_item_drops_non_matching_pair() {
        let out = map_broadcast_item("BTC/USDC", Ok(notif("ETH/USDC")));
        assert!(out.is_none(), "non-matching pair must be dropped");
    }

    #[test]
    fn map_broadcast_item_passes_matching_pair() {
        let out = map_broadcast_item("ETH/USDC", Ok(notif("ETH/USDC")));
        let item = out.expect("matching pair forwarded");
        assert!(item.is_ok());
    }
}
