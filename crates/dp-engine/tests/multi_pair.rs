use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use dp_crypto::{CryptoError, DecryptedOrder, Decrypter};
use dp_engine::{Engine, PairConfig, PairStatus};
use dp_event::MemStore;
use dp_types::{DarkPoolError, Side};
use rust_decimal::Decimal;

struct JsonDecrypter;

impl Decrypter for JsonDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>,
    > {
        Box::pin(async move { serde_json::from_slice(ciphertext).map_err(CryptoError::from) })
    }
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap()
}

fn new_engine() -> Engine {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_millis(50));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    engine
}

/// Distinct trader per placed order. Self-trade prevention keys on the
/// `trader` address (#168), so a crossing bid/ask sharing an address would be
/// skipped as a self-cross and match nothing. `0xEE` prefix scheme mirrors
/// the `dp-auction` and `dp-api` test helpers.
fn next_trader() -> Address {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut bytes = [0u8; 20];
    bytes[0] = 0xEE;
    bytes[12..].copy_from_slice(&n.to_be_bytes());
    Address::from(bytes)
}

async fn place(
    engine: &Engine,
    pair: &str,
    side: Side,
    price: &str,
    size: &str,
    key: &str,
) -> Result<dp_types::Order, dp_engine::EngineError> {
    let d = DecryptedOrder {
        trader: next_trader(),
        pair: pair.into(),
        side,
        price: dec(price),
        size: dec(size),
        commitment_key: key.into(),
        ttl: 60_000_000_000,
    };
    let ct = serde_json::to_vec(&d).unwrap();
    engine
        .place_encrypted_order(vec![0u8; 32], vec![], ct, None)
        .await
}

#[tokio::test]
async fn place_rejects_unregistered_pair() {
    let engine = new_engine();
    let err = place(&engine, "ETH/USDC", Side::Buy, "1800", "1", "k")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("pair not registered"), "got: {msg}");
}

#[tokio::test]
async fn place_rejects_suspended_pair() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine.suspend_pair("ETH/USDC").unwrap();
    let err = place(&engine, "ETH/USDC", Side::Buy, "1800", "1", "k")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("suspended"), "got: {msg}");
}

#[tokio::test]
async fn place_rejects_below_min_order_size() {
    let engine = new_engine();
    let cfg = PairConfig {
        base_token: Address::ZERO,
        quote_token: Address::ZERO,
        base_decimals: 18,
        quote_decimals: 18,
        min_order_size: dec("1"),
        tick_size: Decimal::ZERO,
        auction_interval: None,
        status: PairStatus::Active,
    };
    engine.register_pair_with_event("ETH/USDC", cfg).unwrap();
    let err = place(&engine, "ETH/USDC", Side::Buy, "1800", "0.5", "k")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("below minimum"), "got: {msg}");
}

#[tokio::test]
async fn place_rejects_off_tick_price() {
    let engine = new_engine();
    let cfg = PairConfig {
        base_token: Address::ZERO,
        quote_token: Address::ZERO,
        base_decimals: 18,
        quote_decimals: 18,
        min_order_size: Decimal::ZERO,
        tick_size: dec("0.05"),
        auction_interval: None,
        status: PairStatus::Active,
    };
    engine.register_pair_with_event("ETH/USDC", cfg).unwrap();
    let err = place(&engine, "ETH/USDC", Side::Buy, "1800.07", "1", "k")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not on tick"), "got: {msg}");
    // Price aligned with tick goes through.
    place(&engine, "ETH/USDC", Side::Buy, "1800.05", "1", "k")
        .await
        .expect("on-tick price should be accepted");
}

#[tokio::test]
async fn pairs_match_independently_in_one_tick() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine
        .register_pair_with_event("BTC/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();

    // Crossing orders for each pair, but ETH bid is below BTC ask and
    // vice-versa — so a cross-pair scan would erroneously see no matches.
    place(&engine, "ETH/USDC", Side::Buy, "1800", "2", "eth-bid")
        .await
        .unwrap();
    place(&engine, "ETH/USDC", Side::Sell, "1800", "2", "eth-ask")
        .await
        .unwrap();
    place(&engine, "BTC/USDC", Side::Buy, "60000", "1", "btc-bid")
        .await
        .unwrap();
    place(&engine, "BTC/USDC", Side::Sell, "60000", "1", "btc-ask")
        .await
        .unwrap();

    let mut notifs = engine.run_auction_tick().await;
    notifs.sort_by(|a, b| a.pair.cmp(&b.pair));
    assert_eq!(notifs.len(), 2);
    assert_eq!(notifs[0].pair, "BTC/USDC");
    assert_eq!(notifs[0].clearing_price, dec("60000"));
    assert_eq!(notifs[0].matched_volume, dec("1"));
    assert_eq!(notifs[1].pair, "ETH/USDC");
    assert_eq!(notifs[1].clearing_price, dec("1800"));
    assert_eq!(notifs[1].matched_volume, dec("2"));
}

#[tokio::test]
async fn suspended_pair_does_not_match_but_keeps_book_intact() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    place(&engine, "ETH/USDC", Side::Buy, "1800", "2", "eth-bid")
        .await
        .unwrap();
    place(&engine, "ETH/USDC", Side::Sell, "1800", "2", "eth-ask")
        .await
        .unwrap();
    engine.suspend_pair("ETH/USDC").unwrap();

    let notifs = engine.run_auction_tick().await;
    assert!(notifs.is_empty(), "suspended pair should not match");
    assert_eq!(engine.active_order_count(), 2, "orders stay in the book");
}

#[tokio::test]
async fn delist_cancels_open_orders_and_blocks_new_orders() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    place(&engine, "ETH/USDC", Side::Buy, "1800", "2", "eth-bid")
        .await
        .unwrap();
    place(&engine, "ETH/USDC", Side::Sell, "1900", "2", "eth-ask")
        .await
        .unwrap();
    assert_eq!(engine.active_order_count(), 2);

    let cancelled = engine.delist_pair("ETH/USDC").unwrap();
    assert_eq!(cancelled, 2);
    assert_eq!(engine.active_order_count(), 0);

    let err = place(&engine, "ETH/USDC", Side::Buy, "1800", "1", "x")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        dp_engine::EngineError::Validation(DarkPoolError::PairNotAccepting(_))
    ));
}

#[tokio::test]
async fn pair_registry_replays_from_event_log() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine
        .register_pair_with_event("BTC/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine.suspend_pair("BTC/USDC").unwrap();

    let engine2 = Engine::new(store, Duration::from_millis(50));
    engine2.set_decrypter(Arc::new(JsonDecrypter));
    engine2.recover().await.unwrap();

    assert_eq!(
        engine2.pair_status("ETH/USDC"),
        Some(PairStatus::Active),
        "ETH/USDC should replay as Active"
    );
    assert_eq!(
        engine2.pair_status("BTC/USDC"),
        Some(PairStatus::Suspended),
        "BTC/USDC should replay as Suspended"
    );
    assert!(engine2.pair_status("DOGE/USDC").is_none());
}

#[tokio::test]
async fn suspend_on_already_suspended_is_idempotent_no_duplicate_event() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine.suspend_pair("ETH/USDC").unwrap();
    // Retried admin call: must succeed without writing another
    // PairSuspended event (otherwise replay sees a duplicate).
    engine.suspend_pair("ETH/USDC").unwrap();
    assert_eq!(engine.pair_status("ETH/USDC"), Some(PairStatus::Suspended));
}

#[tokio::test]
async fn suspend_on_delisted_returns_explicit_error() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    engine.delist_pair("ETH/USDC").unwrap();
    let err = engine.suspend_pair("ETH/USDC").unwrap_err();
    assert!(matches!(
        err,
        dp_engine::EngineError::Validation(DarkPoolError::CannotSuspendDelistedPair(_))
    ));
}

#[tokio::test]
async fn delist_on_already_delisted_is_idempotent_returns_zero() {
    let engine = new_engine();
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    let cancelled_first = engine.delist_pair("ETH/USDC").unwrap();
    assert_eq!(cancelled_first, 0);
    // Second call: must not emit another PairDelisted event.
    let cancelled_retry = engine.delist_pair("ETH/USDC").unwrap();
    assert_eq!(cancelled_retry, 0);
    assert_eq!(engine.pair_status("ETH/USDC"), Some(PairStatus::Delisted));
}

/// Regression: recovery used to copy `decrypted.pair` straight into the
/// book Order, so a trader who sent `eth/usdc` ended up keyed under
/// `eth/usdc` post-restart while the registry held the canonical
/// `ETH/USDC`. The auction loop iterates registry keys → orphan.
#[tokio::test]
async fn recovery_canonicalises_trader_cased_pair() {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store.clone(), Duration::from_millis(50));
    engine.set_decrypter(Arc::new(JsonDecrypter));
    engine
        .register_pair_with_event("ETH/USDC", PairConfig::new(Address::ZERO, Address::ZERO))
        .unwrap();
    // Trader sends lowercase casing inside the ciphertext.
    place(&engine, "eth/usdc", Side::Buy, "1800", "1", "ck")
        .await
        .unwrap();

    let engine2 = Engine::new(store, Duration::from_millis(50));
    engine2.set_decrypter(Arc::new(JsonDecrypter));
    engine2.recover().await.unwrap();

    // Book is keyed by the canonical pair; lookup with that key returns
    // the order. Without canonicalisation in recovery this would be empty.
    let (bids, _) = engine2.get_order_book("ETH/USDC");
    assert_eq!(
        bids.len(),
        1,
        "recovered order must land under canonical key"
    );
    assert_eq!(bids[0].pair, "ETH/USDC");
}
