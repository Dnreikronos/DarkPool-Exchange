use std::pin::Pin;

use dp_engine::AuctionNotification;
use dp_engine::Engine;
use futures_util::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::auth::AuthenticatedIdentity;
use crate::conv::{
    notification_to_event, notification_to_summary, order_to_proto, pair_config_to_proto,
    parse_uuid,
};
use crate::error::engine_error_to_status;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    AuctionEvent, CancelOrderRequest, CancelOrderResponse, GetAuctionHistoryRequest,
    GetAuctionHistoryResponse, GetOrderRequest, GetOrderResponse, ListOrdersRequest,
    ListOrdersResponse, ListPairsRequest, ListPairsResponse, PlaceOrderRequest, PlaceOrderResponse,
    StreamAuctionsRequest,
};
use crate::validation::{validate_pair_for_history, validate_pair_known, validate_place_order};

/// Extract the wallet address of the authenticated caller, if any. A
/// SIWE-authenticated trader yields `Some(address)`; an operator API-key
/// caller (or no identity) yields `None`. The per-trader read surfaces
/// (`GetOrder`, `ListOrders`) treat a `None` caller as owning nothing.
fn caller_address<T>(req: &Request<T>) -> Option<alloy_primitives::Address> {
    req.extensions()
        .get::<AuthenticatedIdentity>()
        .and_then(|id| match id {
            AuthenticatedIdentity::Wallet(addr) => Some(*addr),
            AuthenticatedIdentity::ApiKey => None,
        })
}

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
        let caller = caller_address(&req);
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
        let caller = caller_address(&req);
        let req = req.into_inner();
        let id = parse_uuid(&req.order_id)?;
        // Caller-scoped: an order is visible only to its owner. A missing
        // order and an order owned by someone else are deliberately
        // indistinguishable (both `not_found`) so a key-holder cannot probe
        // for the existence of other traders' resting orders.
        let visible = self
            .engine
            .get_order(id)
            .filter(|o| caller == Some(o.trader));
        match visible {
            Some(o) => Ok(Response::new(GetOrderResponse {
                order: Some(order_to_proto(&o)),
            })),
            None => Err(Status::not_found(format!(
                "order {} not found",
                req.order_id
            ))),
        }
    }

    async fn list_orders(
        &self,
        req: Request<ListOrdersRequest>,
    ) -> Result<Response<ListOrdersResponse>, Status> {
        // "My orders": the caller's own resting orders, never anyone
        // else's. Without a wallet identity (e.g. an operator API key) the
        // caller owns nothing → empty list, not the cross-trader book.
        let Some(trader) = caller_address(&req) else {
            return Ok(Response::new(ListOrdersResponse { orders: Vec::new() }));
        };
        let req = req.into_inner();
        let pair_filter = if req.pair.is_empty() {
            None
        } else {
            Some(validate_pair_known(&self.engine, &req.pair)?)
        };
        let orders = self
            .engine
            .orders_by_trader(trader)
            .into_iter()
            .filter(|o| pair_filter.as_ref().is_none_or(|p| o.pair == p.as_str()))
            .map(|o| order_to_proto(&o))
            .collect();
        Ok(Response::new(ListOrdersResponse { orders }))
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
