use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Extension, MatchedPath, Path, Query, State};
use axum::http::{header, HeaderName, HeaderValue, Method, Request as AxumRequest, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Code, Request};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::trace::TraceLayer;

use crate::admin::{AdminApiHandler, AdminKeyError, KeyAdminHandler};
use crate::auth::{auth_axum_mw, AuthCore, AuthenticatedIdentity};
use crate::handler::ApiHandler;
use crate::pb::dark_pool_admin_service_server::DarkPoolAdminService;
use crate::pb::dark_pool_service_server::DarkPoolService;
use crate::pb::{
    self, CancelOrderRequest, DelistPairRequest, GetAuctionHistoryRequest, GetOrderRequest,
    ListOrdersRequest, ListPairsAdminRequest, ListPairsRequest, PlaceOrderRequest,
    RegisterPairRequest, SuspendPairRequest,
};
use crate::ratelimit::{ratelimit_axum_mw, ratelimit_ip_axum_mw, ClientKey, RateLimitCore};
use crate::readiness::ReadinessProbes;
use crate::siwe::SiweState;
use crate::validation::{validate_pair_known, MAX_CIPHERTEXT_BYTES, MAX_PROOF_BYTES};
use dp_crypto::{KeyStatus, MultiKeyDecrypter};

pub type SharedHandler = Arc<ApiHandler>;
pub type SharedAdminHandler = Arc<AdminApiHandler>;
pub type SharedKeyAdminHandler = Arc<KeyAdminHandler>;

#[derive(Clone)]
struct MakeRequestUlid;

impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(&mut self, _request: &AxumRequest<B>) -> Option<RequestId> {
        let id = ulid::Ulid::new().to_string();
        Some(RequestId::new(HeaderValue::from_str(&id).unwrap()))
    }
}

// Slack above raw byte caps to absorb base64 inflation (~4/3) plus JSON envelope.
// Rejects oversized requests before JSON parse + base64 decode burn CPU.
const PLACE_ORDER_BODY_LIMIT: usize = (MAX_PROOF_BYTES + MAX_CIPHERTEXT_BYTES) * 2;

pub fn router(handler: SharedHandler) -> Router {
    let place_order =
        post(rest_place_order).layer(RequestBodyLimitLayer::new(PLACE_ORDER_BODY_LIMIT));
    Router::new()
        .route("/v1/orders", place_order.get(rest_list_orders))
        .route(
            "/v1/orders/:order_id",
            delete(rest_cancel_order).get(rest_get_order),
        )
        .route("/v1/auctions", get(rest_get_auction_history))
        .route("/v1/auctions/stream", get(rest_stream_auctions))
        .route("/v1/pairs", get(rest_list_pairs))
        .with_state(handler)
}

pub fn admin_router(handler: SharedAdminHandler) -> Router {
    Router::new()
        .route(
            "/v1/admin/pairs",
            post(rest_register_pair).get(rest_list_pairs_admin),
        )
        .route("/v1/admin/pairs/:pair/suspend", patch(rest_suspend_pair))
        .route("/v1/admin/pairs/:pair", delete(rest_delist_pair))
        .with_state(handler)
}

/// Key-rotation admin routes. Separate from `admin_router` because the
/// state type differs (`KeyAdminHandler` vs `AdminApiHandler`) — axum's
/// `with_state` is monomorphised per-router so merging them is the
/// idiomatic way to keep both state types reachable from the same
/// listener.
pub fn key_admin_router(handler: SharedKeyAdminHandler) -> Router {
    Router::new()
        .route(
            "/v1/admin/keys",
            post(rest_register_key).get(rest_list_keys),
        )
        .route("/v1/admin/keys/:key_id", delete(rest_delete_key))
        .route("/v1/admin/keys/:key_id/sunset", post(rest_sunset_key))
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

/// Mount public + admin routes on the same listener. Public routes use
/// the trader API keys (`AuthCore`); admin routes use the separate
/// operator key set (`admin_auth`). The rate limiter is shared.
///
/// `admin_auth` must be non-empty unless the operator has explicitly
/// opted into unauthenticated admin: an empty [`AuthCore`] authenticates
/// every request, so a future non-`main` entrypoint that builds this
/// router must first run
/// [`crate::config::Config::validate_admin_auth`] (as `main` does) or
/// the admin endpoints fail open.
pub fn router_with_admin(
    handler: SharedHandler,
    admin_handler: SharedAdminHandler,
    key_admin_handler: SharedKeyAdminHandler,
    auth: AuthCore,
    admin_auth: AuthCore,
    ratelimit: RateLimitCore,
) -> Router {
    let public = router(handler)
        .layer(from_fn_with_state(ratelimit.clone(), ratelimit_axum_mw))
        .layer(from_fn_with_state(auth, auth_axum_mw));
    let admin = admin_router(admin_handler)
        .merge(key_admin_router(key_admin_handler))
        .layer(from_fn_with_state(ratelimit, ratelimit_axum_mw))
        .layer(from_fn_with_state(admin_auth, auth_axum_mw));
    public.merge(admin)
}

/// State carried by the operational endpoints (`/healthz`, `/readyz`,
/// `/metrics`). Cheap to clone — all fields are `Arc`-wrapped internally.
#[derive(Clone)]
pub struct OpsState {
    pub prom: PrometheusHandle,
    pub readiness: ReadinessProbes,
    pub multi: MultiKeyDecrypter,
}

/// Build the unauthenticated operational sub-router. Mounted alongside
/// the auth-gated public + admin routers; load-balancer probes and the
/// Prometheus scrape do not present API keys, so these paths must sit
/// outside the auth layer.
pub fn ops_router(state: OpsState) -> Router {
    Router::new()
        .route("/healthz", get(rest_healthz))
        .route("/readyz", get(rest_readyz))
        .route("/metrics", get(rest_metrics))
        .route("/v1/operator/pubkey", get(rest_operator_pubkey))
        .with_state(state)
}

pub fn auth_router(state: SiweState) -> Router {
    Router::new()
        .route("/v1/auth/nonce", get(rest_auth_nonce))
        .route("/v1/auth/verify", post(rest_auth_verify))
        .with_state(state)
}

/// Compose the full server router: public + admin (auth-gated) + ops
/// (unauthenticated) and apply tracing + x-request-id propagation
/// across every route so logs and spans share a correlation ID.
#[allow(clippy::too_many_arguments)]
pub fn router_with_ops(
    handler: SharedHandler,
    admin_handler: SharedAdminHandler,
    key_admin_handler: SharedKeyAdminHandler,
    auth: AuthCore,
    admin_auth: AuthCore,
    ratelimit: RateLimitCore,
    ops: OpsState,
    cors_origins: &[String],
    siwe_state: Option<SiweState>,
) -> Router {
    let mut base = router_with_admin(
        handler,
        admin_handler,
        key_admin_handler,
        auth,
        admin_auth,
        ratelimit.clone(),
    )
    // The auth (SIWE) and ops sub-routers sit outside the auth layer, so
    // they get their own IP-keyed rate limiter — without it they would be
    // an unthrottled login-DoS surface (issue #159). Keying is by peer IP
    // only; the `x-api-key` header is untrusted on these routes.
    .merge(ops_router(ops).layer(from_fn_with_state(ratelimit.clone(), ratelimit_ip_axum_mw)));
    if let Some(siwe) = siwe_state {
        base = base.merge(
            auth_router(siwe).layer(from_fn_with_state(ratelimit.clone(), ratelimit_ip_axum_mw)),
        );
    }
    // Layers are added bottom-up but execute top-down: the LAST `.layer(...)`
    // wraps everything before it and therefore runs first on the request
    // path. Desired request-side order is SetRequestId → Trace → Propagate,
    // so we call them here in the reverse order — propagate (innermost),
    // trace, then SetRequestId (outermost, generates the id before
    // TraceLayer reads it).
    let traced = base
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http().make_span_with(make_http_span))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUlid));

    if cors_origins.is_empty() {
        return traced;
    }

    let origins: Vec<HeaderValue> = cors_origins
        .iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!(origin = %o, "ignoring unparseable CORS origin");
                None
            }
        })
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("x-api-key"),
        ])
        .expose_headers([HeaderName::from_static("x-request-id")])
        .max_age(Duration::from_secs(3600));

    traced.layer(cors)
}

/// Build the per-request tracing span. Pulls the request_id off the
/// header set by `SetRequestIdLayer` so log lines (and OTel spans) all
/// carry the same correlation token.
fn make_http_span(request: &AxumRequest<Body>) -> tracing::Span {
    // Prefer the matched route template (e.g. `/v1/orders/:order_id`) so
    // span/log telemetry does not explode in cardinality for ID-bearing
    // paths. Fall back to the raw URI when no route matched (the ops
    // sub-router and unknown paths).
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or_else(|| request.uri().path());
    let req_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    tracing::info_span!(
        "http",
        method = %request.method(),
        path = %path,
        request_id = req_id,
    )
}

// ---------- SIWE auth handlers ----------

#[derive(Serialize)]
struct NonceResponse {
    nonce: String,
}

#[derive(Deserialize)]
struct VerifyRequest {
    message: String,
    signature: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    token: String,
    expires_at: u64,
    address: String,
}

async fn rest_auth_nonce(
    State(state): State<SiweState>,
    client: Option<Extension<ClientKey>>,
) -> Result<Json<NonceResponse>, ApiError> {
    // `ratelimit_ip_axum_mw` records the caller's IP key; fall back to the
    // shared anonymous budget when that middleware isn't in the stack
    // (e.g. unit tests hitting `auth_router` directly).
    let key = client
        .map(|Extension(c)| c.0)
        .unwrap_or_else(|| "anonymous".to_string());
    let nonce = state
        .nonce_store
        .generate_for(&key)
        .ok_or_else(|| ApiError(tonic::Status::resource_exhausted("nonce store full")))?;
    Ok(Json(NonceResponse { nonce }))
}

async fn rest_auth_verify(
    State(state): State<SiweState>,
    Json(body): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let sig_bytes = hex::decode(body.signature.strip_prefix("0x").unwrap_or(&body.signature))
        .map_err(|_| ApiError(tonic::Status::invalid_argument("invalid signature hex")))?;
    let sig: [u8; 65] = sig_bytes.try_into().map_err(|_| {
        ApiError(tonic::Status::invalid_argument(
            "signature must be 65 bytes",
        ))
    })?;
    let address = crate::siwe::verify_siwe_message(
        &body.message,
        &sig,
        &state.nonce_store,
        state.chain_id,
        state.expected_domain.as_deref(),
    )
    .map_err(|e| ApiError(tonic::Status::unauthenticated(e.to_string())))?;

    let (token, expires_at) = state
        .jwt_manager
        .issue(address)
        .map_err(|e| ApiError(tonic::Status::internal(format!("jwt issue: {e}"))))?;

    Ok(Json(VerifyResponse {
        token,
        expires_at,
        address: format!("{:#x}", address),
    }))
}

// ---------- ops handlers ----------

async fn rest_healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

async fn rest_readyz(State(state): State<OpsState>) -> Response {
    match state.readiness.check() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "ready"}))).into_response(),
        Err(f) => {
            // `/readyz` is unauthenticated; surfacing the probe's `reason`
            // can leak internal paths or backend errors to anyone able to
            // hit the listener. Log it server-side and keep the response
            // body to a stable name → operators correlate via logs.
            tracing::warn!(probe = f.name, reason = %f.reason, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "status": "not_ready",
                    "failed": f.name,
                })),
            )
                .into_response()
        }
    }
}

async fn rest_metrics(State(state): State<OpsState>) -> Response {
    let body = state.prom.render();
    let mut resp = Response::new(Body::from(body));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    resp
}

#[derive(Serialize)]
struct PubkeyResponse {
    pubkey: String,
    encoding: &'static str,
}

async fn rest_operator_pubkey(State(state): State<OpsState>) -> Response {
    match state.multi.active_public_key_hex() {
        Some(pk) => {
            let body = Json(PubkeyResponse {
                pubkey: pk,
                encoding: "sec1-uncompressed",
            });
            let mut resp = (StatusCode::OK, body).into_response();
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            );
            resp
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "code": 14,
                "message": "no active operator key configured"
            })),
        )
            .into_response(),
    }
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
struct ListOrdersRespJson {
    orders: Vec<OrderInfoJson>,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairInfoJson {
    pair: String,
    base_token: String,
    quote_token: String,
    min_order_size: String,
    tick_size: String,
    auction_interval_ms: Option<u32>,
    status: String,
}

impl From<pb::PairInfo> for PairInfoJson {
    fn from(p: pb::PairInfo) -> Self {
        let status = match pb::PairStatus::try_from(p.status) {
            Ok(pb::PairStatus::Active) => "PAIR_STATUS_ACTIVE",
            Ok(pb::PairStatus::Suspended) => "PAIR_STATUS_SUSPENDED",
            Ok(pb::PairStatus::Delisted) => "PAIR_STATUS_DELISTED",
            _ => "PAIR_STATUS_UNSPECIFIED",
        };
        Self {
            pair: p.pair,
            base_token: p.base_token,
            quote_token: p.quote_token,
            min_order_size: p.min_order_size,
            tick_size: p.tick_size,
            auction_interval_ms: p.auction_interval_ms,
            status: status.to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListPairsJson {
    pairs: Vec<PairInfoJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterPairJson {
    pair: String,
    base_token: String,
    quote_token: String,
    #[serde(default)]
    min_order_size: String,
    #[serde(default)]
    tick_size: String,
    #[serde(default)]
    auction_interval_ms: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisterPairRespJson {
    pair: Option<PairInfoJson>,
}

#[derive(Serialize)]
struct SuspendPairRespJson {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DelistPairRespJson {
    cancelled_orders: u32,
}

#[derive(Deserialize)]
struct CancelQuery {
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
struct ListOrdersQuery {
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

#[derive(Deserialize)]
struct AuctionStreamQuery {
    #[serde(default)]
    pair: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuctionEventSseJson {
    auction_id: String,
    pair: String,
    clearing_price: String,
    matched_volume: String,
    match_count: u32,
    timestamp_unix: String,
}

impl From<dp_engine::AuctionNotification> for AuctionEventSseJson {
    fn from(n: dp_engine::AuctionNotification) -> Self {
        Self {
            auction_id: n.auction_id.to_string(),
            pair: n.pair,
            clearing_price: n.clearing_price.to_string(),
            matched_volume: n.matched_volume.to_string(),
            match_count: n.match_count,
            timestamp_unix: n.timestamp.timestamp().to_string(),
        }
    }
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

/// Carry the authenticated identity injected by `auth_axum_mw` (an axum
/// request extension) into the tonic `Request` the gRPC handler reads. The
/// REST layer builds a fresh tonic request, so without this the per-trader
/// scoping in `place_order` / `cancel_order` / `get_order` / `list_orders`
/// would never see the caller. Absent on routers mounted without the auth
/// middleware.
fn with_identity<T>(inner: T, identity: Option<Extension<AuthenticatedIdentity>>) -> Request<T> {
    let mut req = Request::new(inner);
    if let Some(Extension(id)) = identity {
        req.extensions_mut().insert(id);
    }
    req
}

async fn rest_place_order(
    State(h): State<SharedHandler>,
    identity: Option<Extension<AuthenticatedIdentity>>,
    Json(body): Json<PlaceOrderJson>,
) -> Result<Json<PlaceOrderRespJson>, ApiError> {
    let req = with_identity(
        PlaceOrderRequest {
            commitment: body.commitment,
            proof: body.proof,
            encrypted_payload: body.encrypted_payload,
        },
        identity,
    );
    let resp = h.place_order(req).await?.into_inner();
    Ok(Json(PlaceOrderRespJson {
        order: resp.order.map(Into::into),
    }))
}

async fn rest_cancel_order(
    State(h): State<SharedHandler>,
    identity: Option<Extension<AuthenticatedIdentity>>,
    Path(order_id): Path<String>,
    Query(q): Query<CancelQuery>,
) -> Result<Json<CancelOrderRespJson>, ApiError> {
    let req = with_identity(
        CancelOrderRequest {
            order_id,
            reason: q.reason,
        },
        identity,
    );
    h.cancel_order(req).await?;
    Ok(Json(CancelOrderRespJson {}))
}

async fn rest_get_order(
    State(h): State<SharedHandler>,
    identity: Option<Extension<AuthenticatedIdentity>>,
    Path(order_id): Path<String>,
) -> Result<Json<GetOrderRespJson>, ApiError> {
    let req = with_identity(GetOrderRequest { order_id }, identity);
    let resp = h.get_order(req).await?.into_inner();
    Ok(Json(GetOrderRespJson {
        order: resp.order.map(Into::into),
    }))
}

/// `GET /v1/orders` — the caller's own resting orders ("my orders").
/// Scoped to the authenticated wallet; there is no public order book.
async fn rest_list_orders(
    State(h): State<SharedHandler>,
    identity: Option<Extension<AuthenticatedIdentity>>,
    Query(q): Query<ListOrdersQuery>,
) -> Result<Json<ListOrdersRespJson>, ApiError> {
    let req = with_identity(ListOrdersRequest { pair: q.pair }, identity);
    let resp = h.list_orders(req).await?.into_inner();
    Ok(Json(ListOrdersRespJson {
        orders: resp.orders.into_iter().map(Into::into).collect(),
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

async fn rest_stream_auctions(
    State(h): State<SharedHandler>,
    Query(q): Query<AuctionStreamQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let pair = if q.pair.is_empty() {
        String::new()
    } else {
        validate_pair_known(&h.engine, &q.pair)?.into_string()
    };
    let rx = h.engine.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let pair = pair.clone();
        async move {
            match item {
                Ok(n) => {
                    if !pair.is_empty() && n.pair != pair {
                        return None;
                    }
                    let json: AuctionEventSseJson = n.into();
                    let data = serde_json::to_string(&json)
                        .expect("AuctionEventSseJson is always serializable");
                    Some(Ok(Event::default().event("auction").data(data)))
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    let data = format!(r#"{{"lagged":{}}}"#, n);
                    Some(Ok(Event::default().event("error").data(data)))
                }
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

async fn rest_list_pairs(State(h): State<SharedHandler>) -> Result<Json<ListPairsJson>, ApiError> {
    let resp = h
        .list_pairs(Request::new(ListPairsRequest {}))
        .await?
        .into_inner();
    Ok(Json(ListPairsJson {
        pairs: resp.pairs.into_iter().map(Into::into).collect(),
    }))
}

async fn rest_register_pair(
    State(h): State<SharedAdminHandler>,
    Json(body): Json<RegisterPairJson>,
) -> Result<Json<RegisterPairRespJson>, ApiError> {
    let req = RegisterPairRequest {
        pair: body.pair,
        base_token: body.base_token,
        quote_token: body.quote_token,
        min_order_size: body.min_order_size,
        tick_size: body.tick_size,
        auction_interval_ms: body.auction_interval_ms,
    };
    let resp = h.register_pair(Request::new(req)).await?.into_inner();
    Ok(Json(RegisterPairRespJson {
        pair: resp.pair.map(Into::into),
    }))
}

async fn rest_suspend_pair(
    State(h): State<SharedAdminHandler>,
    Path(pair): Path<String>,
) -> Result<Json<SuspendPairRespJson>, ApiError> {
    let req = SuspendPairRequest { pair };
    h.suspend_pair(Request::new(req)).await?;
    Ok(Json(SuspendPairRespJson {}))
}

async fn rest_delist_pair(
    State(h): State<SharedAdminHandler>,
    Path(pair): Path<String>,
) -> Result<Json<DelistPairRespJson>, ApiError> {
    let req = DelistPairRequest { pair };
    let resp = h.delist_pair(Request::new(req)).await?.into_inner();
    Ok(Json(DelistPairRespJson {
        cancelled_orders: resp.cancelled_orders,
    }))
}

async fn rest_list_pairs_admin(
    State(h): State<SharedAdminHandler>,
) -> Result<Json<ListPairsJson>, ApiError> {
    let resp = h
        .list_pairs_admin(Request::new(ListPairsAdminRequest {}))
        .await?
        .into_inner();
    Ok(Json(ListPairsJson {
        pairs: resp.pairs.into_iter().map(Into::into).collect(),
    }))
}

// ---------- key-rotation admin handlers ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterKeyJson {
    id: String,
    uri: String,
    /// Optional, defaults to `rotating` so a careless `POST` does not
    /// silently demote the current `active` key. Operators promote by
    /// re-POSTing the active id with `status="active"`.
    #[serde(default)]
    status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyInfoJson {
    id: String,
    status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListKeysRespJson {
    keys: Vec<KeyInfoJson>,
}

fn parse_status(s: Option<&str>) -> Result<KeyStatus, ApiError> {
    match s.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("active") => Ok(KeyStatus::Active),
        Some("rotating") | None | Some("") => Ok(KeyStatus::Rotating),
        Some("sunset") => Ok(KeyStatus::Sunset),
        Some(other) => Err(ApiError(tonic::Status::invalid_argument(format!(
            "invalid status: {other} (expected active|rotating|sunset)"
        )))),
    }
}

fn key_admin_err(e: AdminKeyError) -> ApiError {
    use tonic::Status;
    match e {
        AdminKeyError::Invalid(m) => ApiError(Status::invalid_argument(m)),
        AdminKeyError::NotFound(m) => ApiError(Status::not_found(m)),
        AdminKeyError::Resolve(m) => ApiError(Status::failed_precondition(m)),
    }
}

async fn rest_register_key(
    State(h): State<SharedKeyAdminHandler>,
    Json(body): Json<RegisterKeyJson>,
) -> Result<Json<KeyInfoJson>, ApiError> {
    let id = body.id.trim().to_owned();
    let uri = body.uri.trim();
    if id.is_empty() {
        return Err(key_admin_err(AdminKeyError::Invalid(
            "id is required".into(),
        )));
    }
    if uri.is_empty() {
        return Err(key_admin_err(AdminKeyError::Invalid(
            "uri is required".into(),
        )));
    }
    let status = parse_status(body.status.as_deref())?;
    // `upsert` does blocking I/O (file read for `file:`, age decrypt,
    // and — once wired — an AWS KMS round-trip for `awskms:`). Run it
    // off the tokio worker so a slow URI cannot stall unrelated
    // requests sharing the worker pool.
    let handler = h.clone();
    let upsert_id = id.clone();
    let upsert_uri = uri.to_owned();
    tokio::task::spawn_blocking(move || handler.upsert(upsert_id, &upsert_uri, status))
        .await
        .map_err(|e| ApiError(tonic::Status::internal(format!("upsert join error: {e}"))))?
        .map_err(key_admin_err)?;
    Ok(Json(KeyInfoJson {
        id,
        status: status.to_string(),
    }))
}

async fn rest_list_keys(State(h): State<SharedKeyAdminHandler>) -> Json<ListKeysRespJson> {
    let keys = h
        .list()
        .into_iter()
        .map(|(id, status)| KeyInfoJson {
            id,
            status: status.to_string(),
        })
        .collect();
    Json(ListKeysRespJson { keys })
}

async fn rest_delete_key(
    State(h): State<SharedKeyAdminHandler>,
    Path(key_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if h.remove(&key_id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(key_admin_err(AdminKeyError::NotFound(key_id)))
    }
}

async fn rest_sunset_key(
    State(h): State<SharedKeyAdminHandler>,
    Path(key_id): Path<String>,
) -> Result<Json<KeyInfoJson>, ApiError> {
    if h.sunset(&key_id) {
        Ok(Json(KeyInfoJson {
            id: key_id,
            status: KeyStatus::Sunset.to_string(),
        }))
    } else {
        Err(key_admin_err(AdminKeyError::NotFound(key_id)))
    }
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
    fn api_error_maps_failed_precondition_to_400() {
        let err = ApiError(tonic::Status::failed_precondition("nope"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn api_error_maps_out_of_range_to_400() {
        let err = ApiError(tonic::Status::out_of_range("range"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn api_error_maps_already_exists_to_409() {
        let err = ApiError(tonic::Status::already_exists("dup"));
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn auction_summary_json_conversion() {
        let a = pb::AuctionSummary {
            auction_id: "id".into(),
            pair: "ETH/USDC".into(),
            clearing_price: "2000".into(),
            matched_volume: "1.5".into(),
            match_count: 2,
            timestamp_unix: 1700000000,
        };
        let json: AuctionSummaryJson = a.into();
        assert_eq!(json.auction_id, "id");
        assert_eq!(json.pair, "ETH/USDC");
        assert_eq!(json.clearing_price, "2000");
        assert_eq!(json.match_count, 2);
        assert_eq!(json.timestamp_unix, "1700000000");
    }

    #[test]
    fn pair_info_json_status_active() {
        let p = pb::PairInfo {
            pair: "ETH/USDC".into(),
            base_token: "0x1".into(),
            quote_token: "0x2".into(),
            min_order_size: "0.01".into(),
            tick_size: "0.01".into(),
            auction_interval_ms: None,
            status: pb::PairStatus::Active as i32,
        };
        let json: PairInfoJson = p.into();
        assert_eq!(json.status, "PAIR_STATUS_ACTIVE");
    }

    #[test]
    fn pair_info_json_status_suspended() {
        let p = pb::PairInfo {
            pair: "ETH/USDC".into(),
            base_token: "0x1".into(),
            quote_token: "0x2".into(),
            min_order_size: "0.01".into(),
            tick_size: "0.01".into(),
            auction_interval_ms: Some(5000),
            status: pb::PairStatus::Suspended as i32,
        };
        let json: PairInfoJson = p.into();
        assert_eq!(json.status, "PAIR_STATUS_SUSPENDED");
        assert_eq!(json.auction_interval_ms, Some(5000));
    }

    #[test]
    fn pair_info_json_status_delisted() {
        let p = pb::PairInfo {
            pair: "ETH/USDC".into(),
            base_token: "0x1".into(),
            quote_token: "0x2".into(),
            min_order_size: "0.01".into(),
            tick_size: "0.01".into(),
            auction_interval_ms: None,
            status: pb::PairStatus::Delisted as i32,
        };
        let json: PairInfoJson = p.into();
        assert_eq!(json.status, "PAIR_STATUS_DELISTED");
    }

    #[test]
    fn pair_info_json_status_unspecified_falls_back() {
        let p = pb::PairInfo {
            pair: "ETH/USDC".into(),
            base_token: "0x1".into(),
            quote_token: "0x2".into(),
            min_order_size: "0.01".into(),
            tick_size: "0.01".into(),
            auction_interval_ms: None,
            status: 99,
        };
        let json: PairInfoJson = p.into();
        assert_eq!(json.status, "PAIR_STATUS_UNSPECIFIED");
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

    // ---------- key-rotation REST coverage ----------

    use crate::admin::{AdminKeyError, KeyAdminHandler};
    use axum::body::to_bytes;
    use axum::http::Request;
    use dp_crypto::MultiKeyDecrypter;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tower::ServiceExt;

    fn write_hex_key_file(content: &[u8; 32]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(hex::encode(content).as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    fn key_admin_app() -> (axum::Router, NamedTempFile) {
        let handler = Arc::new(KeyAdminHandler::new(MultiKeyDecrypter::new()));
        let f = write_hex_key_file(&[0xAB; 32]);
        (key_admin_router(handler), f)
    }

    fn parse_status_ok(s: Option<&str>) -> KeyStatus {
        match parse_status(s) {
            Ok(s) => s,
            Err(_) => panic!("expected Ok for {:?}", s),
        }
    }

    #[test]
    fn parse_status_active() {
        assert_eq!(parse_status_ok(Some("active")), KeyStatus::Active);
        assert_eq!(parse_status_ok(Some("  ACTIVE ")), KeyStatus::Active);
    }

    #[test]
    fn parse_status_rotating_is_default() {
        assert_eq!(parse_status_ok(None), KeyStatus::Rotating);
        assert_eq!(parse_status_ok(Some("")), KeyStatus::Rotating);
        assert_eq!(parse_status_ok(Some("rotating")), KeyStatus::Rotating);
    }

    #[test]
    fn parse_status_sunset() {
        assert_eq!(parse_status_ok(Some("sunset")), KeyStatus::Sunset);
    }

    #[test]
    fn parse_status_invalid_returns_400() {
        let err = match parse_status(Some("retired")) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn key_admin_err_maps_invalid_to_400() {
        let resp = key_admin_err(AdminKeyError::Invalid("bad".into())).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn key_admin_err_maps_not_found_to_404() {
        let resp = key_admin_err(AdminKeyError::NotFound("k".into())).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn key_admin_err_maps_resolve_to_400() {
        let resp = key_admin_err(AdminKeyError::Resolve("nope".into())).into_response();
        // FailedPrecondition maps to BAD_REQUEST in the table above.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    async fn body_bytes(resp: Response) -> Vec<u8> {
        to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body bytes")
            .to_vec()
    }

    #[tokio::test]
    async fn rest_register_key_round_trip() {
        let (app, f) = key_admin_app();
        let uri = format!("file:{}", f.path().display());
        let body = serde_json::json!({"id": "k-2026", "uri": uri, "status": "active"});

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["id"], "k-2026");
        assert_eq!(v["status"], "active");

        // List should now reflect the registered key.
        let list_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/admin/keys")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(list_resp.status(), StatusCode::OK);
        let bytes = body_bytes(list_resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["keys"][0]["id"], "k-2026");
        assert_eq!(v["keys"][0]["status"], "active");
    }

    #[tokio::test]
    async fn rest_register_key_rejects_blank_id() {
        let (app, _f) = key_admin_app();
        let body = serde_json::json!({"id": "  ", "uri": "file:/tmp/x"});
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rest_register_key_rejects_blank_uri() {
        let (app, _f) = key_admin_app();
        let body = serde_json::json!({"id": "k", "uri": "   "});
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rest_register_key_rejects_unknown_scheme() {
        let (app, _f) = key_admin_app();
        let body = serde_json::json!({"id": "k", "uri": "vault://nope"});
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        // Resolve -> FailedPrecondition -> 400.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rest_register_key_rejects_invalid_status() {
        let (app, f) = key_admin_app();
        let uri = format!("file:{}", f.path().display());
        let body = serde_json::json!({"id": "k", "uri": uri, "status": "retired"});
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rest_sunset_key_round_trip() {
        let (app, f) = key_admin_app();
        let uri = format!("file:{}", f.path().display());

        // Register first.
        let body = serde_json::json!({"id": "to-sunset", "uri": uri, "status": "active"});
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("register");
        assert_eq!(resp.status(), StatusCode::OK);

        // Sunset.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys/to-sunset/sunset")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("sunset");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "sunset");
    }

    #[tokio::test]
    async fn rest_sunset_unknown_key_returns_404() {
        let (app, _f) = key_admin_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys/missing/sunset")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rest_delete_key_round_trip() {
        let (app, f) = key_admin_app();
        let uri = format!("file:{}", f.path().display());
        let body = serde_json::json!({"id": "doomed", "uri": uri, "status": "sunset"});
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/keys")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("register");
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/admin/keys/doomed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("delete");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Second delete returns 404 once the entry is gone.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/admin/keys/doomed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("delete-again");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rest_healthz_returns_ok() {
        async fn run() -> Response {
            rest_healthz().await.into_response()
        }
        let resp = run().await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "ok");
    }

    // ---------- operator pubkey endpoint ----------

    use dp_crypto::{EciesDecrypter, KeyEntry};

    fn ops_app(multi: MultiKeyDecrypter) -> axum::Router {
        let prom = metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder()
            .handle();
        let state = OpsState {
            prom,
            readiness: ReadinessProbes::new(),
            multi,
        };
        ops_router(state)
    }

    #[tokio::test]
    async fn rest_operator_pubkey_returns_active_key() {
        let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng())
            .to_bytes()
            .to_vec();
        let dec = EciesDecrypter::from_bytes(sk).unwrap();
        let expected_hex = hex::encode(dec.public_key());
        let multi = MultiKeyDecrypter::from_entries(vec![KeyEntry::new(
            "test",
            dp_crypto::KeyStatus::Active,
            Arc::new(dec),
        )]);
        let app = ops_app(multi);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/operator/pubkey")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("cache-control").unwrap(),
            "public, max-age=300"
        );
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["pubkey"], expected_hex);
        assert_eq!(v["encoding"], "sec1-uncompressed");
    }

    #[tokio::test]
    async fn rest_operator_pubkey_503_when_no_active() {
        let multi = MultiKeyDecrypter::new();
        let app = ops_app(multi);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/operator/pubkey")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], 14);
    }

    #[tokio::test]
    async fn rest_operator_pubkey_roundtrip_encrypt_decrypt() {
        let sk_bytes = {
            let sk = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
            sk.to_bytes().to_vec()
        };
        let dec = EciesDecrypter::from_bytes(sk_bytes.clone()).unwrap();
        let multi = MultiKeyDecrypter::from_entries(vec![KeyEntry::new(
            "rt",
            dp_crypto::KeyStatus::Active,
            Arc::new(dec),
        )]);
        let app = ops_app(multi);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/operator/pubkey")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        let bytes = body_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let pubkey_hex = v["pubkey"].as_str().unwrap();
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let msg = b"test message for encryption roundtrip";
        let ciphertext = ecies::encrypt(&pubkey_bytes, msg).unwrap();
        let plaintext = ecies::decrypt(&sk_bytes, &ciphertext).unwrap();
        assert_eq!(plaintext, msg);
    }
}
