//! ECIES encryption helpers. Operator pubkey is a SEC1 secp256k1 point.

use crate::error::ClientError;
use crate::payload::OrderPayload;

const SEC1_COMPRESSED: usize = 33;
const SEC1_UNCOMPRESSED: usize = 65;

pub fn encrypt_order(operator_pubkey: &[u8], payload: &OrderPayload) -> Result<Vec<u8>, ClientError> {
    validate_pubkey_len(operator_pubkey)?;
    let plaintext = serde_json::to_vec(payload)?;
    ecies::encrypt(operator_pubkey, &plaintext).map_err(|e| ClientError::Encryption(e.to_string()))
}

pub fn parse_operator_pubkey_hex(hex_str: &str) -> Result<Vec<u8>, ClientError> {
    let trimmed = hex_str.trim().trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(trimmed)?;
    validate_pubkey_len(&bytes)?;
    Ok(bytes)
}

#[cfg(feature = "cli")]
pub fn load_operator_pubkey_file(path: &std::path::Path) -> Result<Vec<u8>, ClientError> {
    let raw = std::fs::read_to_string(path)?;
    parse_operator_pubkey_hex(&raw)
}

fn validate_pubkey_len(b: &[u8]) -> Result<(), ClientError> {
    match b.len() {
        SEC1_COMPRESSED | SEC1_UNCOMPRESSED => Ok(()),
        n => Err(ClientError::InvalidKey(format!(
            "expected 33 (compressed) or 65 (uncompressed) SEC1 bytes, got {n}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_0x_prefix() {
        let raw = vec![0x02u8; 33];
        let with = format!("0x{}", hex::encode(&raw));
        let no = hex::encode(&raw);
        assert_eq!(parse_operator_pubkey_hex(&with).unwrap(), raw);
        assert_eq!(parse_operator_pubkey_hex(&no).unwrap(), raw);
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let bad = hex::encode([0u8; 32]);
        assert!(matches!(
            parse_operator_pubkey_hex(&bad),
            Err(ClientError::InvalidKey(_))
        ));
    }

    #[test]
    fn parse_rejects_non_hex() {
        assert!(matches!(
            parse_operator_pubkey_hex("nothex!!"),
            Err(ClientError::Hex(_))
        ));
    }

    #[test]
    fn parse_accepts_uncompressed() {
        let raw = vec![0x04u8; 65];
        assert_eq!(
            parse_operator_pubkey_hex(&hex::encode(&raw)).unwrap(),
            raw
        );
    }
}
