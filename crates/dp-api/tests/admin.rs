use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dp_api::admin::{AdminApiHandler, KeyAdminHandler};
use dp_api::auth::AuthCore;
use dp_api::handler::ApiHandler;
use dp_api::ratelimit::RateLimitCore;
use dp_api::rest;
use dp_crypto::MultiKeyDecrypter;
use dp_engine::Engine;
use dp_event::MemStore;
use http_body_util::BodyExt;
use tower::ServiceExt;

const OPERATOR_KEY: &str = "operator-key";
const TRADER_KEY: &str = "trader-key";

fn make_app() -> axum::Router {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    let handler = ApiHandler::new(engine.clone());
    let admin = AdminApiHandler::new(engine);
    let key_admin = KeyAdminHandler::new(MultiKeyDecrypter::new());

    let auth = AuthCore::new(vec![TRADER_KEY.to_string()]);
    let admin_auth = AuthCore::new(vec![OPERATOR_KEY.to_string()]);
    let rl = RateLimitCore::new(1_000.0, 1_000.0, Duration::from_secs(60));
    rest::router_with_admin(
        Arc::new(handler),
        Arc::new(admin),
        Arc::new(key_admin),
        auth,
        admin_auth,
        rl,
    )
}

async fn body_json(b: Body) -> serde_json::Value {
    let bytes = b.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn register_body() -> String {
    serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "0x0000000000000000000000000000000000000001",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "0.01",
        "tickSize": "0.01"
    })
    .to_string()
}

#[tokio::test]
async fn admin_register_pair_without_operator_key_is_401() {
    let app = make_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_register_pair_with_trader_key_is_401() {
    let app = make_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", TRADER_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_register_pair_succeeds_with_operator_key() {
    let app = make_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["pair"]["pair"], "ETH/USDC");
    assert_eq!(json["pair"]["status"], "PAIR_STATUS_ACTIVE");
    assert_eq!(json["pair"]["minOrderSize"], "0.01");
}

#[tokio::test]
async fn admin_register_duplicate_pair_returns_409() {
    let app = make_app();
    // first register OK
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn admin_suspend_and_list_reflect_status() {
    let app = make_app();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v1/admin/pairs/ETH%2FUSDC/suspend")
                .header("x-api-key", OPERATOR_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Admin listing shows suspended pair.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["pairs"][0]["pair"], "ETH/USDC");
    assert_eq!(json["pairs"][0]["status"], "PAIR_STATUS_SUSPENDED");

    // Public listing hides suspended pairs.
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/pairs")
                .header("x-api-key", TRADER_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    assert!(json["pairs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn admin_delist_returns_cancelled_count() {
    let app = make_app();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/admin/pairs/ETH%2FUSDC")
                .header("x-api-key", OPERATOR_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["cancelledOrders"], 0);
}

#[tokio::test]
async fn public_pairs_endpoint_does_not_require_admin_key() {
    let app = make_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/pairs")
                .header("x-api-key", TRADER_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_register_rejects_negative_min_order_size() {
    let app = make_app();
    let body = serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "0x0000000000000000000000000000000000000001",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "-0.5",
        "tickSize": "0.01"
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_register_propagates_auction_interval_ms() {
    let app = make_app();
    let body = serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "0x0000000000000000000000000000000000000001",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "0.01",
        "tickSize": "0.01",
        "auctionIntervalMs": 3000
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["pair"]["auctionIntervalMs"], 3000);
}

#[tokio::test]
async fn admin_list_pairs_sorts_alphabetically() {
    let app = make_app();
    for pair in ["ZRX/USDC", "BTC/USDC", "ETH/USDC"] {
        let body = serde_json::json!({
            "pair": pair,
            "baseToken": "0x0000000000000000000000000000000000000001",
            "quoteToken": "0x0000000000000000000000000000000000000002",
            "minOrderSize": "0.01",
            "tickSize": "0.01"
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/pairs")
                    .header("x-api-key", OPERATOR_KEY)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    let names: Vec<String> = json["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pair"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["BTC/USDC", "ETH/USDC", "ZRX/USDC"]);
}

#[tokio::test]
async fn admin_register_rejects_bad_base_token() {
    let app = make_app();
    let body = serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "not-an-address",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "0.01",
        "tickSize": "0.01"
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_suspend_rejects_empty_pair() {
    let app = make_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v1/admin/pairs/%20/suspend")
                .header("x-api-key", OPERATOR_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // A whitespace-only pair fails `Pair::parse` with `PairRequired`,
    // which maps to InvalidArgument → 400.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_register_propagates_decimals() {
    let app = make_app();
    let body = serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "0x0000000000000000000000000000000000000001",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "0.01",
        "tickSize": "0.01",
        "baseDecimals": 18,
        "quoteDecimals": 6
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["pair"]["baseDecimals"], 18);
    assert_eq!(json["pair"]["quoteDecimals"], 6);
}

#[tokio::test]
async fn admin_register_rejects_decimals_above_30() {
    let app = make_app();
    let body = serde_json::json!({
        "pair": "ETH/USDC",
        "baseToken": "0x0000000000000000000000000000000000000001",
        "quoteToken": "0x0000000000000000000000000000000000000002",
        "minOrderSize": "0.01",
        "tickSize": "0.01",
        "quoteDecimals": 31
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/pairs")
                .header("x-api-key", OPERATOR_KEY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
