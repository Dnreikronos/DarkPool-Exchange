#[allow(unexpected_cfgs)]
#[path = "../../dp-client/src/encrypt.rs"]
mod encrypt;
#[allow(dead_code, unexpected_cfgs)]
#[path = "../../dp-client/src/error.rs"]
mod error;
#[allow(dead_code)]
#[path = "../../dp-client/src/payload.rs"]
mod payload;

use alloy_primitives::Address;
use dp_crypto::{Decrypter, EciesDecrypter};
use k256::ecdsa::SigningKey;
use rust_decimal::Decimal;

fn pin_non_darkpool_ecies_config() {
    ecies::config::update_config(ecies::config::Config {
        is_ephemeral_key_compressed: true,
        is_hkdf_key_compressed: true,
    });
}

#[test]
fn ecies_dependency_specs_stay_in_lockstep() {
    let root_manifest = include_str!("../../../Cargo.toml");
    let client_manifest = include_str!("../../dp-client/Cargo.toml");
    let expected = r#"ecies = { version = "0.2", default-features = false, features = ["pure"] }"#;

    assert!(
        root_manifest.contains(expected),
        "root workspace ECIES dependency must stay pinned to {expected}"
    );
    assert!(
        client_manifest.contains(expected),
        "dp-client ECIES dependency must stay pinned to {expected}"
    );
}

#[tokio::test]
async fn dp_client_ciphertext_decrypts_after_config_drift() {
    let sk = SigningKey::random(&mut rand::thread_rng());
    let dec = EciesDecrypter::from_bytes(sk.to_bytes().to_vec()).unwrap();
    let payload = payload::OrderPayload {
        trader: Address::ZERO.to_string(),
        pair: "ETH-USD".into(),
        side: payload::Side::Buy,
        price: Decimal::new(250000, 2),
        size: Decimal::new(10, 1),
        commitment_key: "abc123".into(),
        salt: "11".repeat(32),
        ttl: 5_000_000_000,
    };

    pin_non_darkpool_ecies_config();
    let ciphertext = encrypt::encrypt_order(dec.public_key(), &payload).unwrap();

    pin_non_darkpool_ecies_config();
    let decrypted = dec.decrypt(&ciphertext).await.unwrap();

    assert_eq!(decrypted.trader, Address::ZERO);
    assert_eq!(decrypted.pair, payload.pair);
    assert_eq!(decrypted.side, dp_types::Side::Buy);
    assert_eq!(decrypted.price, payload.price);
    assert_eq!(decrypted.size, payload.size);
    assert_eq!(decrypted.commitment_key, payload.commitment_key);
    assert_eq!(decrypted.salt, payload.salt);
    assert_eq!(decrypted.ttl, payload.ttl);
}
