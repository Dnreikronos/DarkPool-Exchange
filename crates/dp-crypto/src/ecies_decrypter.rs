use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use zeroize::Zeroize;

use crate::decrypted_order::DecryptedOrder;
use crate::{CryptoError, Decrypter};

pub struct EciesDecrypter {
    secret_key: Vec<u8>,
}

impl Drop for EciesDecrypter {
    fn drop(&mut self) {
        self.secret_key.zeroize();
    }
}

impl EciesDecrypter {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, CryptoError> {
        let sk = load_operator_key_file(path)?;
        Ok(Self { secret_key: sk })
    }
}

impl Decrypter for EciesDecrypter {
    fn decrypt<'a>(
        &'a self,
        ciphertext: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<DecryptedOrder, CryptoError>> + Send + 'a>> {
        Box::pin(async move {
            let plaintext = ecies::decrypt(&self.secret_key, ciphertext)
                .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;
            let order: DecryptedOrder = serde_json::from_slice(&plaintext)?;
            Ok(order)
        })
    }
}

pub fn load_operator_key_file(path: impl AsRef<Path>) -> Result<Vec<u8>, CryptoError> {
    let raw = std::fs::read_to_string(path)?;
    let trimmed = raw.trim().trim_start_matches("0x");
    let key_bytes = hex::decode(trimmed)?;
    if key_bytes.len() != 32 {
        return Err(CryptoError::InvalidKeyFile(format!(
            "expected 32 bytes, got {}",
            key_bytes.len()
        )));
    }
    Ok(key_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;
    use rust_decimal::Decimal;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn test_order() -> DecryptedOrder {
        DecryptedOrder {
            trader: alloy_primitives::Address::ZERO,
            pair: "ETH-USD".into(),
            side: dp_types::Side::Sell,
            price: Decimal::new(180000, 2),
            size: Decimal::new(5, 0),
            commitment_key: "testkey".into(),
            ttl: 3_000_000_000,
        }
    }

    fn write_key_file(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[tokio::test]
    async fn ecies_roundtrip() {
        let sk = SigningKey::random(&mut rand::thread_rng());
        let sk_bytes = sk.to_bytes();
        let pk = sk.verifying_key();
        let pk_bytes = pk.to_sec1_bytes();

        let order = test_order();
        let plaintext = serde_json::to_vec(&order).unwrap();
        let ciphertext = ecies::encrypt(&pk_bytes, &plaintext).unwrap();

        let dec = EciesDecrypter {
            secret_key: sk_bytes.to_vec(),
        };
        let result = dec.decrypt(&ciphertext).await.unwrap();
        assert_eq!(result, order);
    }

    #[tokio::test]
    async fn tampered_ciphertext_rejected() {
        let sk = SigningKey::random(&mut rand::thread_rng());
        let sk_bytes = sk.to_bytes();
        let pk = sk.verifying_key();
        let pk_bytes = pk.to_sec1_bytes();

        let order = test_order();
        let plaintext = serde_json::to_vec(&order).unwrap();
        let mut ciphertext = ecies::encrypt(&pk_bytes, &plaintext).unwrap();

        let mid = ciphertext.len() / 2;
        ciphertext[mid] ^= 0xff;

        let dec = EciesDecrypter {
            secret_key: sk_bytes.to_vec(),
        };
        let result = dec.decrypt(&ciphertext).await;
        assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
    }

    #[test]
    fn bad_key_not_hex() {
        let f = write_key_file("not-valid-hex!");
        let result = load_operator_key_file(f.path());
        assert!(matches!(result, Err(CryptoError::HexDecode(_))));
    }

    #[test]
    fn bad_key_wrong_length() {
        let f = write_key_file(&hex::encode([0u8; 16]));
        let result = load_operator_key_file(f.path());
        assert!(matches!(result, Err(CryptoError::InvalidKeyFile(_))));
    }

    #[test]
    fn bad_key_missing_file() {
        let result = load_operator_key_file("/tmp/nonexistent_key_dp_crypto_test");
        assert!(matches!(result, Err(CryptoError::Io(_))));
    }

    #[test]
    fn load_key_0x_prefix() {
        let key = [0xab_u8; 32];
        let f = write_key_file(&format!("0x{}", hex::encode(key)));
        let loaded = load_operator_key_file(f.path()).unwrap();
        assert_eq!(loaded, key);
    }

    #[test]
    fn load_key_whitespace() {
        let key = [0xcd_u8; 32];
        let f = write_key_file(&format!("  {}  \n", hex::encode(key)));
        let loaded = load_operator_key_file(f.path()).unwrap();
        assert_eq!(loaded, key);
    }
}
