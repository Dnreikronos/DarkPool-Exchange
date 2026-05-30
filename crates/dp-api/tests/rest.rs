use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use dp_api::auth::AuthenticatedIdentity;
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

// ---------- REST happy paths ----------

fn registered_app() -> (axum::Router, dp_engine::Engine) {
    use std::sync::Arc as ArcSync;
    let store = ArcSync::new(MemStore::new());
    let engine = dp_engine::Engine::new(store, std::time::Duration::from_secs(1));
    engine.set_decrypter(ArcSync::new(JsonDecrypter));
    engine.register_pair_without_event("ETH/USDC".into(), dp_engine::PairConfig::default());
    let handler = dp_api::handler::ApiHandler::new(engine.clone());
    let router = dp_api::rest::router(ArcSync::new(handler));
    (router, engine)
}

struct JsonDecrypter;

impl dp_crypto::Decrypter for JsonDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<dp_crypto::DecryptedOrder, dp_crypto::CryptoError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(
            async move { serde_json::from_slice(ciphertext).map_err(dp_crypto::CryptoError::from) },
        )
    }
}

fn encrypt_b64(order: &dp_crypto::DecryptedOrder) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(serde_json::to_vec(order).unwrap())
}

#[tokio::test]
async fn rest_place_get_cancel_round_trip() {
    let (app, _engine) = registered_app();
    let d = dp_crypto::DecryptedOrder {
        trader: alloy_primitives::Address::ZERO,
        pair: "ETH/USDC".into(),
        side: dp_types::Side::Buy,
        price: "1800".parse().unwrap(),
        size: "1".parse().unwrap(),
        commitment_key: "k1".into(),
        ttl: 60_000_000_000,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let body = serde_json::json!({
        "commitment": STANDARD.encode([0u8; 32]),
        "proof": STANDARD.encode(b"proof"),
        "encryptedPayload": encrypt_b64(&d),
    })
    .to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    let id = json["order"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["order"]["pair"], "ETH/USDC");
    assert_eq!(json["order"]["side"], "SIDE_BUY");

    // GET it back. The order is owned by Address::ZERO; inject a matching
    // wallet identity (normally set by auth_axum_mw) so the caller-scoped
    // handler returns it. Without an identity this would be 404.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/orders/{id}"))
                .extension(AuthenticatedIdentity::Wallet(Address::ZERO))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    assert_eq!(json["order"]["id"], id);

    // Cancel it. Cancel is caller-scoped like GET, so the DELETE carries the
    // owner's wallet identity; without it this would be 404.
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/orders/{id}?reason=testing"))
                .extension(AuthenticatedIdentity::Wallet(Address::ZERO))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// `GET /v1/orders` returns only the authenticated caller's own orders,
/// and nothing at all without a wallet identity. There is no public book.
#[tokio::test]
async fn rest_list_orders_is_scoped_to_caller() {
    let (app, _engine) = registered_app();
    let make_body = |trader: Address, side: dp_types::Side, price: &str| {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let d = dp_crypto::DecryptedOrder {
            trader,
            pair: "ETH/USDC".into(),
            side,
            price: price.parse().unwrap(),
            size: "1".parse().unwrap(),
            commitment_key: format!("k-{}-{}-{}", trader, side as u8, price),
            ttl: 60_000_000_000,
        };
        serde_json::json!({
            "commitment": STANDARD.encode([0u8; 32]),
            "proof": STANDARD.encode(b"p"),
            "encryptedPayload": encrypt_b64(&d),
        })
        .to_string()
    };

    let me = Address::with_last_byte(0x11);
    let other = Address::with_last_byte(0x22);
    // me: a non-crossing buy + sell (both rest); other: one resting buy.
    for (trader, side, price) in [
        (me, dp_types::Side::Buy, "1800"),
        (me, dp_types::Side::Sell, "1850"),
        (other, dp_types::Side::Buy, "1790"),
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(make_body(trader, side, price)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // As `me` → exactly my two orders, never `other`'s.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/orders")
                .extension(AuthenticatedIdentity::Wallet(me))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    assert_eq!(json["orders"].as_array().unwrap().len(), 2);

    // Without a wallet identity → empty list, not the cross-trader book.
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/orders")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_to_json(resp.into_body()).await;
    assert!(json["orders"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn rest_get_order_not_found_returns_404() {
    let (app, _engine) = registered_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/orders/{}", uuid::Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_with_middleware_enforces_api_key() {
    use dp_api::auth::AuthCore;
    use dp_api::ratelimit::RateLimitCore;
    let store = std::sync::Arc::new(MemStore::new());
    let engine = dp_engine::Engine::new(store, std::time::Duration::from_secs(1));
    engine.register_pair_without_event("ETH/USDC".into(), dp_engine::PairConfig::default());
    let handler = dp_api::handler::ApiHandler::new(engine);
    let auth = AuthCore::new(vec!["mw-key".to_string()]);
    let rl = RateLimitCore::new(1_000.0, 1_000.0, std::time::Duration::from_secs(60));
    let app = dp_api::rest::router_with_middleware(std::sync::Arc::new(handler), auth, rl);

    // Missing api key → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/pairs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // With api key → 200
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/pairs")
                .header("x-api-key", "mw-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // With api key in query param → 200
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/pairs?apiKey=mw-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // With percent-encoded api key in query param → 200
    // "mw-key" percent-encoded: "mw%2Dkey"
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/pairs?apiKey=mw%2Dkey")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------- SSE auction stream ----------

#[tokio::test]
async fn sse_stream_unknown_pair_returns_404() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auctions/stream?pair=FAKE/PAIR")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sse_stream_returns_event_stream_content_type() {
    let store = std::sync::Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    engine.register_pair_without_event("ETH/USDC".into(), dp_engine::PairConfig::default());
    let handler = ApiHandler::new(engine);
    let app = rest::router(std::sync::Arc::new(handler));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auctions/stream?pair=ETH/USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"), "content-type was: {ct}");
}

#[tokio::test]
async fn sse_stream_no_pair_filter_returns_200() {
    let app = new_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auctions/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn sse_stream_receives_auction_events() {
    let (app, engine) = registered_app();

    // Place crossing orders so the auction tick produces a match.
    let make_body = |side: dp_types::Side, price: &str| {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let d = dp_crypto::DecryptedOrder {
            trader: alloy_primitives::Address::ZERO,
            pair: "ETH/USDC".into(),
            side,
            price: price.parse().unwrap(),
            size: "1".parse().unwrap(),
            commitment_key: format!("sse-{}-{}", side as u8, price),
            ttl: 60_000_000_000,
        };
        serde_json::json!({
            "commitment": STANDARD.encode([0u8; 32]),
            "proof": STANDARD.encode(b"p"),
            "encryptedPayload": encrypt_b64(&d),
        })
        .to_string()
    };

    for (side, price) in [
        (dp_types::Side::Buy, "1800"),
        (dp_types::Side::Sell, "1800"),
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(make_body(side, price)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/auctions/stream?pair=ETH/USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let mut body = resp.into_body();

    let eng = engine.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        eng.run_auction_tick().await;
    });

    let frame = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buf = String::new();
        loop {
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Some(data) = frame.data_ref() {
                        buf.push_str(&String::from_utf8_lossy(data));
                        if buf.contains("event: auction") {
                            return buf;
                        }
                    }
                }
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await;

    assert!(frame.is_ok(), "timed out waiting for auction SSE event");
    let text = frame.unwrap();
    assert!(text.contains("\"pair\":\"ETH/USDC\""));
    assert!(text.contains("\"clearingPrice\""));
}
