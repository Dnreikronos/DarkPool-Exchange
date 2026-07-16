use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use dp_api::auth::AuthenticatedIdentity;
use dp_api::handler::ApiHandler;
use dp_api::pb::dark_pool_service_server::DarkPoolService;
use dp_api::pb::{
    self, CancelOrderRequest, GetAuctionHistoryRequest, GetOrderRequest, ListOrdersRequest,
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

/// Distinct `trader` address per call. Self-trade prevention keys on `trader`
/// (#168), so a crossing bid/ask placed under one address would be skipped as
/// a self-cross. The `0xEE` prefix keeps these clear of `Address::ZERO` and
/// the small hand-written addresses used elsewhere; the same scheme is
/// mirrored in the `dp-auction` and `dp-engine` test helpers. Tests wanting a
/// shared owner use `place_test_order_as`.
fn next_trader() -> Address {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut bytes = [0u8; 20];
    bytes[0] = 0xEE;
    bytes[12..].copy_from_slice(&n.to_be_bytes());
    Address::from(bytes)
}

fn new_handler() -> ApiHandler {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    engine.register_pair_without_event("ETH/USDC".into(), dp_engine::PairConfig::default());
    ApiHandler::new(engine)
}

fn stub_proof() -> Vec<u8> {
    b"stub-proof".to_vec()
}

fn canonical_salt(n: u64) -> String {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&n.to_be_bytes());
    hex::encode(bytes)
}

fn future_expiry_nanos() -> i64 {
    (chrono::Utc::now() + chrono::Duration::minutes(10))
        .timestamp_nanos_opt()
        .unwrap()
}

fn build_req(d: &DecryptedOrder) -> PlaceOrderRequest {
    let ct = serde_json::to_vec(d).unwrap();
    build_req_from_ciphertext(ct)
}

fn build_req_from_ciphertext(ct: Vec<u8>) -> PlaceOrderRequest {
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
        salt: canonical_salt(n),
        nonce: canonical_salt(n),
        expires_at_unix_nanos: future_expiry_nanos(),
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
        trader: next_trader(),
        pair: pair.to_string(),
        side,
        price: price.parse().unwrap(),
        size: size.parse().unwrap(),
        commitment_key: format!("test-key-{}", n),
        salt: canonical_salt(n),
        nonce: canonical_salt(n),
        expires_at_unix_nanos: future_expiry_nanos(),
        ttl: 600_000_000_000,
    };
    let resp = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .expect("place_order")
        .into_inner();
    resp.order.unwrap().id
}

/// Place an order owned by a specific trader. The request carries the owner's
/// wallet identity so placement satisfies the mandatory trader/caller binding.
async fn place_test_order_as(
    h: &ApiHandler,
    owner: Address,
    pair: &str,
    side: Side,
    price: &str,
    size: &str,
) -> String {
    let n = KEY_COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = DecryptedOrder {
        trader: owner,
        pair: pair.to_string(),
        side,
        price: price.parse().unwrap(),
        size: size.parse().unwrap(),
        commitment_key: format!("test-key-{}", n),
        salt: canonical_salt(n),
        nonce: canonical_salt(n),
        expires_at_unix_nanos: future_expiry_nanos(),
        ttl: 600_000_000_000,
    };
    let resp = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .expect("place_order")
        .into_inner();
    resp.order.unwrap().id
}

/// Wrap a request with a SIWE-style wallet identity so caller-scoped
/// handlers (`get_order`, `list_orders`) see the authenticated trader.
fn wallet_req<T>(inner: T, wallet: Address) -> Request<T> {
    let mut req = Request::new(inner);
    req.extensions_mut()
        .insert(AuthenticatedIdentity::Wallet(wallet));
    req
}

// ---------- PlaceOrder ----------

#[tokio::test]
async fn place_order_success() {
    let h = new_handler();
    let d = valid_decrypted();
    let resp = h
        .place_order(wallet_req(build_req(&d), d.trader))
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
            .place_order(wallet_req(build_req(&d), d.trader))
            .await
            .err()
            .unwrap_or_else(|| panic!("{} should fail", name));
        assert_eq!(err.code(), Code::InvalidArgument, "case: {}", name);
    }
}

#[tokio::test]
async fn place_order_missing_salt_is_invalid_argument() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut v = serde_json::to_value(&d).unwrap();
    v.as_object_mut().unwrap().remove("salt");
    let err = h
        .place_order(wallet_req(
            build_req_from_ciphertext(serde_json::to_vec(&v).unwrap()),
            d.trader,
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn place_order_non_string_salt_is_invalid_argument() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut v = serde_json::to_value(&d).unwrap();
    v["salt"] = serde_json::json!(0);
    let err = h
        .place_order(wallet_req(
            build_req_from_ciphertext(serde_json::to_vec(&v).unwrap()),
            d.trader,
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn place_order_bad_salt_hex_is_invalid_argument() {
    let h = new_handler();
    let mut d = valid_decrypted();
    d.salt = "zz".repeat(32);
    let err = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(err.message().contains("salt"));
}

#[tokio::test]
async fn place_order_wrong_length_salt_is_invalid_argument() {
    let h = new_handler();
    let mut d = valid_decrypted();
    d.salt = "ab".repeat(31);
    let err = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(err.message().contains("salt"));
}

#[tokio::test]
async fn place_order_empty_salt_is_invalid_argument() {
    let h = new_handler();
    let mut d = valid_decrypted();
    d.salt.clear();
    let err = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(err.message().contains("salt"));
}

#[tokio::test]
async fn place_order_uppercase_salt_is_invalid_argument() {
    let h = new_handler();
    let mut d = valid_decrypted();
    d.salt = "AB".repeat(32);
    let err = h
        .place_order(wallet_req(build_req(&d), d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(err.message().contains("salt"));
}

#[tokio::test]
async fn place_order_missing_commitment() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut req = build_req(&d);
    req.commitment.clear();
    let err = h
        .place_order(wallet_req(req, d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_COMMITMENT_REQUIRED);
}

#[tokio::test]
async fn place_order_missing_ciphertext() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut req = build_req(&d);
    req.encrypted_payload.clear();
    let err = h
        .place_order(wallet_req(req, d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_CIPHERTEXT_REQUIRED);
}

#[tokio::test]
async fn place_order_ciphertext_too_large() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut req = build_req(&d);
    req.encrypted_payload = vec![0u8; MAX_CIPHERTEXT_BYTES + 1];
    let err = h
        .place_order(wallet_req(req, d.trader))
        .await
        .err()
        .unwrap();
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
    let resp = h
        .place_order(wallet_req(req, d.trader))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.order.is_some());
}

#[tokio::test]
async fn place_order_missing_proof() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut req = build_req(&d);
    req.proof.clear();
    let err = h
        .place_order(wallet_req(req, d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn place_order_proof_too_large() {
    let h = new_handler();
    let d = valid_decrypted();
    let mut req = build_req(&d);
    req.proof = vec![0u8; MAX_PROOF_BYTES + 1];
    let err = h
        .place_order(wallet_req(req, d.trader))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(err.message(), MSG_PROOF_TOO_LARGE);
}

// ---------- CancelOrder ----------

#[tokio::test]
async fn cancel_order_success() {
    let h = new_handler();
    // Cancel is caller-scoped: the order's owner and the request's wallet
    // identity must be the same address.
    let owner = Address::repeat_byte(3);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    h.cancel_order(wallet_req(
        CancelOrderRequest {
            order_id: id,
            reason: "testing".into(),
        },
        owner,
    ))
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

/// A key-holder must not cancel another trader's order. Like `get_order`,
/// the response is `not_found` (not `permission_denied`) so existence isn't
/// leaked — and crucially the order survives the rejected cancel.
#[tokio::test]
async fn cancel_order_hides_other_traders_order() {
    let h = new_handler();
    let owner = Address::with_last_byte(0x11);
    let attacker = Address::with_last_byte(0x22);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let err = h
        .cancel_order(wallet_req(
            CancelOrderRequest {
                order_id: id.clone(),
                reason: String::new(),
            },
            attacker,
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
    // The order is still live and visible to its real owner.
    let resp = h
        .get_order(wallet_req(GetOrderRequest { order_id: id }, owner))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.order.is_some());
}

/// Without a wallet identity (e.g. an operator API-key caller) no order is
/// cancellable — every CancelOrder is `not_found`, never a cross-trader
/// cancel.
#[tokio::test]
async fn cancel_order_without_wallet_identity_is_not_found() {
    let h = new_handler();
    let owner = Address::with_last_byte(0x11);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let err = h
        .cancel_order(Request::new(CancelOrderRequest {
            order_id: id,
            reason: String::new(),
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
    // GetOrder is caller-scoped: owner and wallet identity must match.
    let owner = Address::repeat_byte(3);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let resp = h
        .get_order(wallet_req(GetOrderRequest { order_id: id }, owner))
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
        .get_order(wallet_req(
            GetOrderRequest {
                order_id: Uuid::new_v4().to_string(),
            },
            Address::ZERO,
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
}

/// A key-holder must not read another trader's order. The response is
/// `not_found` (not `permission_denied`) so existence isn't leaked.
#[tokio::test]
async fn get_order_hides_other_traders_order() {
    let h = new_handler();
    let owner = Address::with_last_byte(0x11);
    let attacker = Address::with_last_byte(0x22);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let err = h
        .get_order(wallet_req(GetOrderRequest { order_id: id }, attacker))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
}

/// Without a wallet identity (e.g. an operator API-key caller) no trader
/// order is visible — every lookup is `not_found`.
#[tokio::test]
async fn get_order_without_wallet_identity_is_not_found() {
    let h = new_handler();
    let owner = Address::with_last_byte(0x11);
    let id = place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let err = h
        .get_order(Request::new(GetOrderRequest { order_id: id }))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code(), Code::NotFound);
}

// ---------- ListOrders (caller-scoped "my orders") ----------

/// The list contains the caller's own orders and never another trader's.
#[tokio::test]
async fn list_orders_returns_only_callers_orders() {
    let h = new_handler();
    let me = Address::with_last_byte(0x11);
    let other = Address::with_last_byte(0x22);
    place_test_order_as(&h, me, "ETH/USDC", Side::Buy, "1800", "5").await;
    place_test_order_as(&h, me, "ETH/USDC", Side::Sell, "1850", "2").await;
    place_test_order_as(&h, other, "ETH/USDC", Side::Buy, "1790", "3").await;
    let resp = h
        .list_orders(wallet_req(
            ListOrdersRequest {
                pair: String::new(),
            },
            me,
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.orders.len(), 2);
}

/// Without a wallet identity the caller owns nothing — empty list, never
/// the cross-trader book.
#[tokio::test]
async fn list_orders_without_wallet_identity_is_empty() {
    let h = new_handler();
    let owner = Address::with_last_byte(0x11);
    place_test_order_as(&h, owner, "ETH/USDC", Side::Buy, "1800", "5").await;
    let resp = h
        .list_orders(Request::new(ListOrdersRequest {
            pair: String::new(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.orders.is_empty());
}

#[tokio::test]
async fn list_orders_filters_by_pair() {
    let h = new_handler();
    let me = Address::with_last_byte(0x11);
    h.engine
        .register_pair_without_event("BTC/USDC".into(), dp_engine::PairConfig::default());
    place_test_order_as(&h, me, "ETH/USDC", Side::Buy, "1800", "5").await;
    place_test_order_as(&h, me, "BTC/USDC", Side::Buy, "60000", "1").await;
    let resp = h
        .list_orders(wallet_req(
            ListOrdersRequest {
                pair: "ETH/USDC".into(),
            },
            me,
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.orders.len(), 1);
    assert_eq!(resp.orders[0].pair, "ETH/USDC");
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

/// Regression: history reads must succeed past a delist because the
/// auction log retains records. Delisting is forward-looking — it stops
/// new orders + auctions but does not redact what already happened.
#[tokio::test]
async fn get_auction_history_succeeds_for_delisted_pair() {
    let h = new_handler();
    place_test_order(&h, "ETH/USDC", Side::Buy, "1850", "5").await;
    place_test_order(&h, "ETH/USDC", Side::Sell, "1800", "5").await;
    h.engine.run_auction_tick().await;
    h.engine
        .delist_pair("ETH/USDC")
        .expect("delist after auction");
    let resp = h
        .get_auction_history(Request::new(GetAuctionHistoryRequest {
            pair: "ETH/USDC".into(),
            limit: 0,
        }))
        .await
        .expect("history readable post-delist")
        .into_inner();
    assert_eq!(resp.auctions.len(), 1);
}

// ---------- StreamAuctions ----------

// ---------- ListPairs ----------

#[tokio::test]
async fn list_pairs_returns_only_active() {
    let h = new_handler();
    h.engine
        .register_pair_without_event("BTC/USDC".into(), dp_engine::PairConfig::default());
    let cfg = dp_engine::PairConfig {
        status: dp_engine::PairStatus::Suspended,
        ..Default::default()
    };
    h.engine
        .register_pair_without_event("DOGE/USDC".into(), cfg);
    let resp = h
        .list_pairs(Request::new(pb::ListPairsRequest::default()))
        .await
        .unwrap()
        .into_inner();
    let names: Vec<&str> = resp.pairs.iter().map(|p| p.pair.as_str()).collect();
    assert!(names.contains(&"ETH/USDC"));
    assert!(names.contains(&"BTC/USDC"));
    assert!(!names.contains(&"DOGE/USDC"));
}

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
async fn stream_auctions_ends_after_configured_timeout() {
    let h = new_handler().with_auction_stream_timeout(Duration::from_millis(10));
    let mut stream = h
        .stream_auctions(Request::new(StreamAuctionsRequest { pair: "".into() }))
        .await
        .unwrap()
        .into_inner();

    let item = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap();
    assert!(item.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_auctions_filters_by_pair() {
    let h = new_handler();
    h.engine
        .register_pair_without_event("BTC/USDC".into(), dp_engine::PairConfig::default());
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
