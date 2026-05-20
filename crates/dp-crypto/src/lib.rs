mod decrypted_order;
mod decrypter;
mod ecies_decrypter;
mod multi_key_decrypter;

pub use decrypted_order::DecryptedOrder;
pub use decrypter::{Decrypter, NoopDecrypter};
pub use ecies_decrypter::{load_operator_key_file, EciesDecrypter};
pub use multi_key_decrypter::{KeyEntry, KeyStatus, MultiKeyDecrypter};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("invalid key file: {0}")]
    InvalidKeyFile(String),

    #[error("deserialization failed: {0}")]
    DeserializationFailed(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
}
