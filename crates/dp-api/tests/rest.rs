use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dp_api::handler::ApiHandler;
use dp_api::rest;
use dp_engine::Engine;
use dp_event::MemStore;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn new_app() -> axum::Router {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    let handler = ApiHandler::new(engine);
    rest::router(Arc::new(handler))
}

async fn body_to_json(b: Body) -> serde_json::Value {
    let bytes = b.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn orderbook_empty_pair_400() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/orderbook")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn orderbook_unknown_pair_returns_empty() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/orderbook?pair=ETH/USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    assert_eq!(json["pair"], "ETH/USDC");
    assert!(json["bids"].as_array().unwrap().is_empty());
    assert!(json["asks"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn auctions_empty() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auctions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    assert!(json["auctions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_order_invalid_uuid_400() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/orders/not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn place_order_missing_fields_400() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"commitment":"","proof":"","encryptedPayload":""}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
