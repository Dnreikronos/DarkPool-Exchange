use std::future::Future;
use std::pin::Pin;

use crate::decrypted_order::DecryptedOrder;
use crate::CryptoError;

pub trait Decrypter: Send + Sync {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>>;
}

pub struct NoopDecrypter;

impl Decrypter for NoopDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>> {
        Box::pin(async move {
            let order: DecryptedOrder = serde_json::from_slice(ciphertext)?;
            Ok(order)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[tokio::test]
    async fn noop_json_roundtrip() {
        let order = DecryptedOrder {
            pair: "BTC-USD".into(),
            side: dp_types::Side::Buy,
            price: Decimal::new(5000000, 2),
            size: Decimal::new(1, 0),
            commitment_key: "key1".into(),
            ttl: 10_000_000_000,
        };
        let json = serde_json::to_vec(&order).unwrap();
        let dec = NoopDecrypter;
        let result = dec.decrypt(&json).await.unwrap();
        assert_eq!(result, order);
    }

    #[tokio::test]
    async fn noop_invalid_json() {
        let dec = NoopDecrypter;
        let result = dec.decrypt(b"not json").await;
        assert!(result.is_err());
    }
}
