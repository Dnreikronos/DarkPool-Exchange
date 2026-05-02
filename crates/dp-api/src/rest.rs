use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tonic::{Code, Request};

use crate::auth::{auth_axum_mw, AuthCore};
use crate::handler::ApiHandler;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    self, CancelOrderRequest, GetAuctionHistoryRequest, GetOrderBookRequest, GetOrderRequest,
    PlaceOrderRequest,
};
use crate::ratelimit::{ratelimit_axum_mw, RateLimitCore};

pub type SharedHandler = Arc<ApiHandler>;

pub fn router(handler: SharedHandler) -> Router {
    Router::new()
        .route("/v1/orders", post(rest_place_order))
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
        _ => "SIDE_UNSPECIFIED".to_string(),
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
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        decode(&s).map_err(serde::de::Error::custom)
    }

    fn decode(input: &str) -> Result<Vec<u8>, String> {
        let bytes = input.as_bytes();
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        if !bytes.len().is_multiple_of(4) {
            return Err("invalid base64 length".into());
        }
        let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
        let val = |c: u8| -> Result<u8, String> {
            Ok(match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return Err(format!("invalid base64 char: {}", c as char)),
            })
        };
        let total = bytes.len();
        for (idx, chunk) in bytes.chunks(4).enumerate() {
            let is_last = idx * 4 + 4 == total;
            // '=' is only valid in the final 4-byte group at positions 2 and/or 3.
            // Reject padding in any other position.
            for (pos, &c) in chunk.iter().enumerate() {
                if c == b'=' && !(is_last && pos >= 2) {
                    return Err("invalid base64 padding".into());
                }
            }
            let v0 = val(chunk[0])?;
            let v1 = val(chunk[1])?;
            let pad2 = chunk[2] == b'=';
            let pad3 = chunk[3] == b'=';
            // '=' at chunk[2] requires '=' at chunk[3].
            if pad2 && !pad3 {
                return Err("invalid base64 padding".into());
            }
            let v2 = if pad2 { 0 } else { val(chunk[2])? };
            let v3 = if pad3 { 0 } else { val(chunk[3])? };
            out.push((v0 << 2) | (v1 >> 4));
            if !pad2 {
                out.push((v1 << 4) | (v2 >> 2));
            }
            if !pad3 {
                out.push((v2 << 6) | v3);
            }
        }
        Ok(out)
    }
}
