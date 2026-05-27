use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("encoding error: {0}")]
    Encoding(String),

    #[error("invalid operator key: {0}")]
    InvalidKey(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("hex decode: {0}")]
    Hex(#[from] hex::FromHexError),

    #[cfg(feature = "cli")]
    #[error("http error: {0}")]
    Http(String),
}
