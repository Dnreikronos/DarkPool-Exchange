mod decrypted_order;
mod decrypter;
mod ecies_decrypter;
mod key_source;
mod multi_key_decrypter;
mod snapshot_cipher;

pub use decrypted_order::DecryptedOrder;
pub use decrypter::{Decrypter, NoopDecrypter};
pub use ecies_decrypter::{load_operator_key_file, EciesDecrypter};
pub use key_source::{decrypter_from_uri, KeySource};
pub use snapshot_cipher::{SnapshotCipher, SNAPSHOT_NONCE_LEN};
pub use multi_key_decrypter::{
    validate_key_id, KeyEntry, KeyStatus, MultiKeyDecrypter, MAX_KEY_ID_LEN,
};

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

    #[error("key source error: {0}")]
    KeySource(String),
}
