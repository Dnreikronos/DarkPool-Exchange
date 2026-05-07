use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use dp_api::handler::ApiHandler;
use dp_api::pb::dark_pool_service_server::DarkPoolService;
use dp_api::pb::{
    self, CancelOrderRequest, GetAuctionHistoryRequest, GetOrderBookRequest, GetOrderRequest,
    PlaceOrderRequest, StreamAuctionsRequest,
};
use dp_api::validation::{
    MAX_CIPHERTEXT_BYTES, MAX_PROOF_BYTES, MSG_CIPHERTEXT_REQUIRED, MSG_CIPHERTEXT_TOO_LARGE,
    MSG_COMMITMENT_REQUIRED, MSG_PROOF_TOO_LARGE,
};
use dp_crypto::DecryptedOrder;
use dp_engine::Engine;
use dp_event::MemStore;
use dp_types::Side;
use rust_decimal::Decimal;
use tokio_stream::StreamExt;
use tonic::{Code, Request};
use uuid::Uuid;

static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_handler() -> ApiHandler {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    ApiHandler::new(engine)
}

fn stub_proof() -> Vec<u8> {
    b"stub-proof".to_vec()
}

fn build_req(d: &DecryptedOrder) -> PlaceOrderRequest {
    let ct = serde_json::to_vec(d).unwrap();
    PlaceOrderRequest {
        // Engine recomputes the canonical Poseidon commitment over decrypted
        // fields; the API only checks commitment is non-empty. Any 32-byte
        // placeholder satisfies the wire-level validation.
        commitment: vec![0u8; 32],
        proof: stub_proof(),
        encrypted_payload: ct,
    }
}

fn valid_decrypted() -> DecryptedOrder {
    let n = KEY_COUNTER.fetch_add(1, Ordering::SeqCst);
    DecryptedOrder {
        trader: Address::ZERO,
        pair: "ETH/USDC".to_string(),
        side: Side::Buy,
        price: Decimal::new(180050, 2),
        size: Decimal::from(10),
        commitment_key: format!("ck-{}", n),
        ttl: 60_000_000_000,
    }
}

async fn place_test_order(
    h: &ApiHandler,
    pair: &str,
    side: Side,
    price: &str,
    size: &str,
) -> String {
    let n = KEY_COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = DecryptedOrder {
        trader: Address::ZERO,
        pair: pair.to_string(),
        side,
        price: price.parse().unwrap(),
        size: size.parse().unwrap(),
        commitment_key: format!("test-key-{}", n),
        ttl: 600_000_000_000,
    };
    let resp = h
        .place_order(Request::new(build_req(&d)))
        .await
        .expect("place_order")
        .into_inner();
    resp.order.unwrap().id
}

// ---------- PlaceOrder ----------

#[tokio::test]
async fn place_order_success() {
    let h = new_handler();
    let resp = h
        .place_order(Request::new(build_req(&valid_decrypted())))
        .await
        .unwrap()
        .into_inner();
    let o = resp.order.expect("order");
    assert_eq!(o.pair, "ETH/USDC");
    assert_eq!(o.side, pb::Side::Buy as i32);
    assert!(o.price == "1800.5" || o.price == "1800.50");
    Uuid::parse_str(&o.id).expect("id is uuid");
}

#[tokio::test]
async fn place_order_validation_errors() {
    type Mut = Box<dyn Fn(&mut DecryptedOrder)>;
    let cases: Vec<(&str, Mut)> = vec![
        ("empty pair", Box::new(|d| d.pair.clear())),
        ("zero price", Box::new(|d| d.price = Decimal::ZERO)),
        ("negative size", Box::new(|d| d.size = Decimal::from(-1))),
        ("empty key", Box::new(|d| d.commitment_key.clear())),
    ];
    for (name, mutate) in cases {
        let h = new_handler();
        let mut d = valid_decrypted();
        mutate(&mut d);
        let err = h
            .place_order(Request::new(build_req(&d)))
            .await
            .err()
            .unwrap_or_else(|| panic!("{} should fail", name));
        assert_eq!(err.code(), Code::InvalidArgument, "case: {}", name);
    }
}

#[tokio::test]
async fn place_order_missing_commitment() {
    let h = new_handler();
    let mut req = build_req(&valid_decrypted());
    req.commitment.clear();
    let err = h.place_order(Request::new(req)).await.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_COMMITMENT_REQUIRED);
}

#[tokio::test]
async fn place_order_missing_ciphertext() {
    let h = new_handler();
    let mut req = build_req(&valid_decrypted());
    req.encrypted_payload.clear();
    let err = h.place_order(Request::new(req)).await.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_CIPHERTEXT_REQUIRED);
}

#[tokio::test]
async fn place_order_ciphertext_too_large() {
    let h = new_handler();
    let mut req = build_req(&valid_decrypted());
    req.encrypted_payload = vec![0u8; MAX_CIPHERTEXT_BYTES + 1];
    let err = h.place_order(Request::new(req)).await.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_CIPHERTEXT_TOO_LARGE);
}

#[tokio::test]
async fn place_order_accepts_any_nonempty_commitment() {
    // The engine derives the canonical Poseidon commitment itself, so the
    // client-supplied commitment field is no longer content-checked. Any
    // non-empty value is accepted by the API and silently overwritten by
    // the engine before persistence.
    let h = new_handler();
    let d = valid_decrypted();
    let ct = serde_json::to_vec(&d).unwrap();
    let req = PlaceOrderRequest {
        commitment: vec![0xAB; 32],
        proof: stub_proof(),
        encrypted_payload: ct,
    };
    let resp = h.place_order(Request::new(req)).await.unwrap().into_inner();
    assert!(resp.order.is_some());
}

#[tokio::test]
async fn place_order_missing_proof() {
    let h = new_handler();
    let mut req = build_req(&valid_decrypted());
    req.proof.clear();
    let err = h.place_order(Request::new(req)).await.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn place_order_proof_too_large() {
    let h = new_handler();
    let mut req = build_req(&valid_decrypted());
    req.proof = vec![0u8; MAX_PROOF_BYTES + 1];
    let err = h.place_order(Request::new(req)).await.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_PROOF_TOO_LARGE);
}

// ---------- CancelOrder ----------

#[tokio::test]
async fn cancel_order_success() {
    let h = new_handler();
    let id = place_test_order(&h, "ETH/USDC", Side::Buy, "1800", "5").await;
    h.cancel_order(Request::new(CancelOrderRequest {
        order_id: id,
        reason: "testing".into(),
    }))
    .await
    .unwrap();
}

#[tokio::test]
async fn cancel_order_invalid_uuid() {
    let h = new_handler();
    let err = h
        .cancel_order(Request::new(CancelOrderRequest {
            order_id: "not-a-uuid".into(),
            reason: "".into(),
        }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn cancel_order_not_found() {
    let h = new_handler();
    let err = h
        .cancel_order(Request::new(CancelOrderRequest {
            order_id: Uuid::new_v4().to_string(),
            reason: "".into(),
        }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
}

// ---------- GetOrder ----------

#[tokio::test]
async fn get_order_success() {
    let h = new_handler();
    let id = place_test_order(&h, "ETH/USDC", Side::Buy, "1800", "5").await;
    let resp = h
        .get_order(Request::new(GetOrderRequest { order_id: id }))
        .await
        .unwrap()
        .into_inner();
    let o = resp.order.unwrap();
    assert_eq!(o.pair, "ETH/USDC");
    assert_eq!(o.side, pb::Side::Buy as i32);
}

#[tokio::test]
async fn get_order_invalid_uuid() {
    let h = new_handler();
    let err = h
        .get_order(Request::new(GetOrderRequest {
            order_id: "bad".into(),
        }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn get_order_not_found() {
    let h = new_handler();
    let err = h
        .get_order(Request::new(GetOrderRequest {
            order_id: Uuid::new_v4().to_string(),
        }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
}

// ---------- GetOrderBook ----------

#[tokio::test]
async fn get_order_book_success() {
    let h = new_handler();
    place_test_order(&h, "ETH/USDC", Side::Buy, "1800", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Buy, "1790", "3").await;
    place_test_order(&h, "ETH/USDC", Side::Sell, "1850", "2").await;
    let resp = h
        .get_order_book(Request::new(GetOrderBookRequest {
            pair: "ETH/USDC".into(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.pair, "ETH/USDC");
    assert_eq!(resp.bids.len(), 2);
    assert_eq!(resp.asks.len(), 1);
}

#[tokio::test]
async fn get_order_book_empty_pair() {
    let h = new_handler();
    let err = h
        .get_order_book(Request::new(GetOrderBookRequest { pair: "".into() }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn get_order_book_aggregation() {
    let h = new_handler();
    place_test_order(&h, "ETH/USDC", Side::Buy, "100", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Buy, "100", "3").await;
    place_test_order(&h, "ETH/USDC", Side::Buy, "200", "10").await;
    let resp = h
        .get_order_book(Request::new(GetOrderBookRequest {
            pair: "ETH/USDC".into(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.bids.len(), 2);
    let lvl = resp.bids.iter().find(|l| l.price == "100").expect("100");
    assert_eq!(lvl.total_size, "8");
    assert_eq!(lvl.order_count, 2);
}

// ---------- GetAuctionHistory ----------

#[tokio::test]
async fn get_auction_history_empty() {
    let h = new_handler();
    let resp = h
        .get_auction_history(Request::new(GetAuctionHistoryRequest::default()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.auctions.len(), 0);
}

#[tokio::test]
async fn get_auction_history_with_auction() {
    let h = new_handler();
    place_test_order(&h, "ETH/USDC", Side::Buy, "1850", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Sell, "1800", "5").await;
    h.engine.run_auction_tick().await;
    let resp = h
        .get_auction_history(Request::new(GetAuctionHistoryRequest {
            pair: "ETH/USDC".into(),
            limit: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.auctions.len(), 1);
    let a = &resp.auctions[0];
    assert_eq!(a.pair, "ETH/USDC");
    assert!(a.match_count >= 1);
}

// ---------- StreamAuctions ----------

#[tokio::test(flavor = "multi_thread")]
async fn stream_auctions_receives_event() {
    let h = new_handler();
    let stream = h
        .stream_auctions(Request::new(StreamAuctionsRequest { pair: "".into() }))
        .await
        .unwrap()
        .into_inner();

    place_test_order(&h, "ETH/USDC", Side::Buy, "1850", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Sell, "1800", "5").await;

    let engine = h.engine.clone();
    let tick = tokio::spawn(async move { engine.run_auction_tick().await });

    let mut count = 0usize;
    let mut stream = stream;
    let recv = async {
        while let Some(item) = stream.next().await {
            if item.is_ok() {
                count += 1;
                break;
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), recv).await;
    let _ = tick.await;
    assert!(count >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_auctions_filters_by_pair() {
    let h = new_handler();
    let stream = h
        .stream_auctions(Request::new(StreamAuctionsRequest {
            pair: "ETH/USDC".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    place_test_order(&h, "ETH/USDC", Side::Buy, "1850", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Sell, "1800", "5").await;
    place_test_order(&h, "BTC/USDC", Side::Buy, "60000", "1").await;
    place_test_order(&h, "BTC/USDC", Side::Sell, "59000", "1").await;

    let engine = h.engine.clone();
    let tick = tokio::spawn(async move { engine.run_auction_tick().await });

    let mut events: Vec<pb::AuctionEvent> = Vec::new();
    let mut stream = stream;
    let recv = async {
        for _ in 0..2 {
            if let Some(Ok(e)) = stream.next().await {
                events.push(e);
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), recv).await;
    let _ = tick.await;
    assert!(
        !events.is_empty(),
        "expected at least one ETH/USDC auction event"
    );
    for e in &events {
        assert_eq!(e.pair, "ETH/USDC");
    }
}
