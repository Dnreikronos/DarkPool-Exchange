//! Cross-crate interop: lock down byte-level agreement with the engine-side
//! crates the client must mirror.
//!
//!  - `dp_crypto::EciesDecrypter` must successfully decrypt the client's
//!    ciphertext into the engine's `DecryptedOrder`.
//!  - `dp_zk::pedersen` Poseidon commitments / trader-id derivations must
//!    equal the client's copy field-element-for-field-element.
//!  - `dp_zk::encoding::decimal_to_scalar` must equal the client's encoder
//!    over the legal range.

use ark_bn254::Fr;
use k256::ecdsa::SigningKey;
use rust_decimal::Decimal;

use dp_client::commitment::{
    bytes_to_scalar as client_bytes_to_scalar, commit_native as client_commit,
    derive_trader_id as client_derive_trader_id, OrderCommitmentInput as ClientCommitmentInput,
};
use dp_client::encoding::decimal_to_scalar as client_decimal_to_scalar;
use dp_client::encrypt::encrypt_order;
use dp_client::payload::{OrderPayload, Side};

use dp_crypto::{Decrypter, EciesDecrypter};
use dp_zk::encoding::decimal_to_scalar as zk_decimal_to_scalar;
use dp_zk::pedersen::{
    commit_native as zk_commit, derive_trader_id as zk_derive_trader_id,
    OrderCommitmentInput as ZkCommitmentInput,
};

#[tokio::test]
async fn ecies_round_trip_decrypts_to_engine_struct() {
    let sk = SigningKey::random(&mut rand::thread_rng());
    let sk_bytes = sk.to_bytes();
    let pk = sk.verifying_key().to_sec1_bytes();

    let payload = OrderPayload {
        pair: "ETH-USD".into(),
        side: Side::Sell,
        price: Decimal::new(180000, 2),
        size: Decimal::new(5, 0),
        commitment_key: "interop-key".into(),
        ttl: 3_000_000_000,
    };

    let ciphertext = encrypt_order(&pk, &payload).unwrap();

    // Engine-side decrypter materialised from the same secret key. The hex
    // hop also covers the `0x`-prefix path through `EciesDecrypter::from_file`.
    let key_hex = format!("0x{}", hex::encode(sk_bytes));
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    tmp.write_all(key_hex.as_bytes()).unwrap();
    tmp.flush().unwrap();
    let dec = EciesDecrypter::from_file(tmp.path()).unwrap();
    let recovered = dec.decrypt(&ciphertext).await.unwrap();

    assert_eq!(recovered.pair, payload.pair);
    assert_eq!(recovered.price, payload.price);
    assert_eq!(recovered.size, payload.size);
    assert_eq!(recovered.commitment_key, payload.commitment_key);
    assert_eq!(recovered.ttl, payload.ttl);
    assert_eq!(recovered.side, dp_types::Side::Sell);
}

#[test]
fn poseidon_commit_matches_dp_zk() {
    let trader_key = b"alice";
    let salt_bytes = [0xab; 32];

    let client_trader = client_derive_trader_id(trader_key);
    let zk_trader = zk_derive_trader_id(trader_key).unwrap();
    assert_eq!(client_trader, zk_trader, "trader_id mismatch");

    let salt_fr = client_bytes_to_scalar(&salt_bytes);

    for (side_u8, price_d, size_d) in [
        (0u8, Decimal::from(100), Decimal::from(10)),
        (1u8, Decimal::new(250000, 2), Decimal::new(15, 1)),
        (0u8, Decimal::ZERO, Decimal::ZERO),
    ] {
        let c_in =
            ClientCommitmentInput::from_decimals(client_trader, side_u8, price_d, size_d, salt_fr)
                .unwrap();
        let z_in =
            ZkCommitmentInput::from_decimals(zk_trader, side_u8, price_d, size_d, salt_fr).unwrap();
        assert_eq!(
            client_commit(&c_in),
            zk_commit(&z_in),
            "commit_native diverges at side={side_u8} price={price_d} size={size_d}"
        );
    }
}

#[test]
fn decimal_encoding_matches_dp_zk() {
    let cases: &[Decimal] = &[
        Decimal::ZERO,
        Decimal::from(1),
        Decimal::from(100),
        Decimal::new(12_345, 2),
        Decimal::new(99_999_999, 8),
        Decimal::new(1, 8),
    ];
    for d in cases {
        let a: Fr = client_decimal_to_scalar(*d).unwrap();
        let b: Fr = zk_decimal_to_scalar(*d).unwrap();
        assert_eq!(a, b, "decimal_to_scalar diverges at {d}");
    }
}
