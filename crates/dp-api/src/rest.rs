use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tonic::{Code, Request};
use tower_http::limit::RequestBodyLimitLayer;

use crate::auth::{auth_axum_mw, AuthCore};
use crate::handler::ApiHandler;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    self, CancelOrderRequest, GetAuctionHistoryRequest, GetOrderBookRequest, GetOrderRequest,
    PlaceOrderRequest,
};
use crate::ratelimit::{ratelimit_axum_mw, RateLimitCore};
use crate::validation::{MAX_CIPHERTEXT_BYTES, MAX_PROOF_BYTES};

pub type SharedHandler = Arc<ApiHandler>;

// Slack above raw byte caps to absorb base64 inflation (~4/3) plus JSON envelope.
// Rejects oversized requests before JSON parse + base64 decode burn CPU.
const PLACE_ORDER_BODY_LIMIT: usize = (MAX_PROOF_BYTES + MAX_CIPHERTEXT_BYTES) * 2;

pub fn router(handler: SharedHandler) -> Router {
    let place_order =
        post(rest_place_order).layer(RequestBodyLimitLayer::new(PLACE_ORDER_BODY_LIMIT));
    Router::new()
        .route("/v1/orders", place_order)
        .route(
            "/v1/orders/:order_id",
            delete(rest_cancel_order).get(rest_get_order),
        )
        .route("/v1/orderbook", get(rest_get_orderbook))
        .route("/v1/auctions", get(rest_get_auction_history))
        .with_state(handler)
}

pub fn router_with_middleware(
    handler: SharedHandler,
    auth: AuthCore,
    ratelimit: RateLimitCore,
) -> Router {
    router(handler)
        .layer(from_fn_with_state(ratelimit, ratelimit_axum_mw))
        .layer(from_fn_with_state(auth, auth_axum_mw))
}

pub fn status_to_response(status: tonic::Status) -> Response {
    ApiError(status).into_response()
}

// ---------- DTOs (camelCase to match grpc-gateway proto3 JSON mapping) ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOrderJson {
    #[serde(default, with = "base64_bytes")]
    commitment: Vec<u8>,
    #[serde(default, with = "base64_bytes")]
    proof: Vec<u8>,
    #[serde(default, with = "base64_bytes")]
    encrypted_payload: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderInfoJson {
    id: String,
    pair: String,
    side: String,
    price: String,
    size: String,
    remaining_size: String,
    commitment_key: String,
    submitted_at_unix: String,
    expires_at_unix: String,
}

impl From<pb::OrderInfo> for OrderInfoJson {
    fn from(o: pb::OrderInfo) -> Self {
        Self {
            side: side_str(o.side),
            id: o.id,
            pair: o.pair,
            price: o.price,
            size: o.size,
            remaining_size: o.remaining_size,
            commitment_key: o.commitment_key,
            submitted_at_unix: o.submitted_at_unix.to_string(),
            expires_at_unix: o.expires_at_unix.to_string(),
        }
    }
}

fn side_str(s: i32) -> String {
    match pb::Side::try_from(s) {
        Ok(pb::Side::Buy) => "SIDE_BUY".to_string(),
        Ok(pb::Side::Sell) => "SIDE_SELL".to_string(),
        Ok(pb::Side::Unspecified) | Err(_) => "SIDE_UNSPECIFIED".to_string(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOrderRespJson {
    order: Option<OrderInfoJson>,
}

#[derive(Serialize)]
struct CancelOrderRespJson {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetOrderRespJson {
    order: Option<OrderInfoJson>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PriceLevelJson {
    price: String,
    total_size: String,
    order_count: i32,
}

impl From<pb::PriceLevel> for PriceLevelJson {
    fn from(l: pb::PriceLevel) -> Self {
        Self {
            price: l.price,
            total_size: l.total_size,
            order_count: l.order_count,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderBookJson {
    pair: String,
    bids: Vec<PriceLevelJson>,
    asks: Vec<PriceLevelJson>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuctionSummaryJson {
    auction_id: String,
    pair: String,
    clearing_price: String,
    matched_volume: String,
    match_count: i32,
    timestamp_unix: String,
}

impl From<pb::AuctionSummary> for AuctionSummaryJson {
    fn from(a: pb::AuctionSummary) -> Self {
        Self {
            auction_id: a.auction_id,
            pair: a.pair,
            clearing_price: a.clearing_price,
            matched_volume: a.matched_volume,
            match_count: a.match_count,
            timestamp_unix: a.timestamp_unix.to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuctionHistoryRespJson {
    auctions: Vec<AuctionSummaryJson>,
}

#[derive(Deserialize)]
struct CancelQuery {
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
struct OrderBookQuery {
    #[serde(default)]
    pair: String,
}

#[derive(Deserialize)]
struct AuctionHistoryQuery {
    #[serde(default)]
    pair: String,
    #[serde(default)]
    limit: i32,
}

// ---------- error mapping ----------

struct ApiError(tonic::Status);

impl From<tonic::Status> for ApiError {
    fn from(s: tonic::Status) -> Self {
        ApiError(s)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = match self.0.code() {
            Code::InvalidArgument => StatusCode::BAD_REQUEST,
            Code::NotFound => StatusCode::NOT_FOUND,
            Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            Code::PermissionDenied => StatusCode::FORBIDDEN,
            Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
            Code::AlreadyExists => StatusCode::CONFLICT,
            Code::FailedPrecondition => StatusCode::BAD_REQUEST,
            Code::OutOfRange => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = serde_json::json!({
            "code": self.0.code() as i32,
            "message": self.0.message(),
        });
        (code, Json(body)).into_response()
    }
}

// ---------- handlers ----------

async fn rest_place_order(
    State(h): State<SharedHandler>,
    Json(body): Json<PlaceOrderJson>,
) -> Result<Json<PlaceOrderRespJson>, ApiError> {
    let req = PlaceOrderRequest {
        commitment: body.commitment,
        proof: body.proof,
        encrypted_payload: body.encrypted_payload,
    };
    let resp = h.place_order(Request::new(req)).await?.into_inner();
    Ok(Json(PlaceOrderRespJson {
        order: resp.order.map(Into::into),
    }))
}

async fn rest_cancel_order(
    State(h): State<SharedHandler>,
    Path(order_id): Path<String>,
    Query(q): Query<CancelQuery>,
) -> Result<Json<CancelOrderRespJson>, ApiError> {
    let req = CancelOrderRequest {
        order_id,
        reason: q.reason,
    };
    h.cancel_order(Request::new(req)).await?;
    Ok(Json(CancelOrderRespJson {}))
}

async fn rest_get_order(
    State(h): State<SharedHandler>,
    Path(order_id): Path<String>,
) -> Result<Json<GetOrderRespJson>, ApiError> {
    let req = GetOrderRequest { order_id };
    let resp = h.get_order(Request::new(req)).await?.into_inner();
    Ok(Json(GetOrderRespJson {
        order: resp.order.map(Into::into),
    }))
}

async fn rest_get_orderbook(
    State(h): State<SharedHandler>,
    Query(q): Query<OrderBookQuery>,
) -> Result<Json<OrderBookJson>, ApiError> {
    let req = GetOrderBookRequest { pair: q.pair };
    let resp = h.get_order_book(Request::new(req)).await?.into_inner();
    Ok(Json(OrderBookJson {
        pair: resp.pair,
        bids: resp.bids.into_iter().map(Into::into).collect(),
        asks: resp.asks.into_iter().map(Into::into).collect(),
    }))
}

async fn rest_get_auction_history(
    State(h): State<SharedHandler>,
    Query(q): Query<AuctionHistoryQuery>,
) -> Result<Json<AuctionHistoryRespJson>, ApiError> {
    let req = GetAuctionHistoryRequest {
        pair: q.pair,
        limit: q.limit,
    };
    let resp = h.get_auction_history(Request::new(req)).await?.into_inner();
    Ok(Json(AuctionHistoryRespJson {
        auctions: resp.auctions.into_iter().map(Into::into).collect(),
    }))
}

// ---------- base64 codec for JSON ----------

mod base64_bytes {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty() {
            return Ok(Vec::new());
        }
        STANDARD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn side_str_buy() {
        assert_eq!(side_str(pb::Side::Buy as i32), "SIDE_BUY");
    }

    #[test]
    fn side_str_sell() {
        assert_eq!(side_str(pb::Side::Sell as i32), "SIDE_SELL");
    }

    #[test]
    fn side_str_unspecified() {
        assert_eq!(side_str(pb::Side::Unspecified as i32), "SIDE_UNSPECIFIED");
    }

    #[test]
    fn side_str_invalid_falls_back() {
        assert_eq!(side_str(999), "SIDE_UNSPECIFIED");
    }

    #[test]
    fn api_error_maps_invalid_argument_to_400() {
        let err = ApiError(tonic::Status::invalid_argument("bad"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn api_error_maps_not_found_to_404() {
        let err = ApiError(tonic::Status::not_found("gone"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn api_error_maps_unauthenticated_to_401() {
        let err = ApiError(tonic::Status::unauthenticated("no auth"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn api_error_maps_permission_denied_to_403() {
        let err = ApiError(tonic::Status::permission_denied("nope"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn api_error_maps_resource_exhausted_to_429() {
        let err = ApiError(tonic::Status::resource_exhausted("slow down"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn api_error_maps_internal_to_500() {
        let err = ApiError(tonic::Status::internal("oops"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn base64_deserialize_empty_string() {
        let json = serde_json::json!("");
        let result: Vec<u8> =
            base64_bytes::deserialize(json).expect("empty string should produce empty vec");
        assert!(result.is_empty());
    }

    #[test]
    fn base64_deserialize_valid() {
        let json = serde_json::json!("aGVsbG8=");
        let result: Vec<u8> = base64_bytes::deserialize(json).expect("valid base64 should decode");
        assert_eq!(result, b"hello");
    }

    #[test]
    fn base64_deserialize_invalid() {
        let json = serde_json::json!("!!!invalid!!!");
        let result: Result<Vec<u8>, _> = base64_bytes::deserialize(json);
        assert!(result.is_err());
    }

    #[test]
    fn order_info_json_conversion() {
        let info = pb::OrderInfo {
            id: "abc".into(),
            pair: "BTC/USD".into(),
            side: pb::Side::Buy as i32,
            price: "100.5".into(),
            size: "1.0".into(),
            remaining_size: "0.5".into(),
            commitment_key: "ck".into(),
            submitted_at_unix: 1000,
            expires_at_unix: 2000,
        };
        let json: OrderInfoJson = info.into();
        assert_eq!(json.side, "SIDE_BUY");
        assert_eq!(json.pair, "BTC/USD");
        assert_eq!(json.submitted_at_unix, "1000");
        assert_eq!(json.expires_at_unix, "2000");
    }
}
