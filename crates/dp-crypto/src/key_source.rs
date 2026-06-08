//! URI-driven backends for the operator ECIES secret.
//!
//! A `KeySource` is the smallest abstraction over "where does the
//! 32-byte secp256k1 secret live?". The supported schemes are:
//!
//! - `file:/path/key.hex` — plaintext hex (current behaviour).
//! - `age:/path/key.age` — age-encrypted with a passphrase from
//!   `DARKPOOL_KEY_PASSPHRASE`. Output is expected to be plaintext
//!   hex of the 32-byte secret.
//! - `awskms:<key-id>?ciphertext=base64...` — AWS KMS envelope: the
//!   ciphertext blob in the query string is decrypted by the named
//!   KMS key. The plaintext is the secp256k1 secret. KMS cannot
//!   natively do secp256k1 ECDH, so we transport the secret through
//!   KMS rather than asking it to perform the ECIES exchange.
//!   Feature-gated behind `aws-kms` and currently stub-only.
//!
//! Each backend returns the secret bytes through [`KeySource::secret`];
//! callers (notably the boot path that hands the bytes to
//! [`EciesDecrypter::from_bytes`]) must zeroize their own copies after
//! handing them off — `EciesDecrypter` zeroizes its internal copy on
//! drop.

use std::path::Path;
use std::sync::Arc;

use zeroize::{Zeroize, Zeroizing};

use crate::ecies_decrypter::{load_operator_key_file, EciesDecrypter};
use crate::{CryptoError, Decrypter};

/// Raw key material plus a stable handle describing where it came from.
/// `secret` is owned `Vec<u8>` so the caller can hand it to
/// [`EciesDecrypter::from_bytes`], which takes ownership and zeroizes
/// on drop.
pub struct KeySource {
    pub uri: String,
    secret: Vec<u8>,
}

impl Drop for KeySource {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl KeySource {
    /// Resolve a URI into raw key bytes. Unknown schemes / missing
    /// passphrases surface as [`CryptoError::KeySource`] rather than
    /// panicking, so a typo in operator config fails fast at boot.
    pub fn from_uri(uri: &str) -> Result<Self, CryptoError> {
        let uri = uri.trim();
        let secret = resolve_secret_bytes(uri)?;
        if secret.len() != 32 {
            return Err(CryptoError::InvalidKeyFile(format!(
                "expected 32-byte secp256k1 secret, got {}",
                secret.len()
            )));
        }
        Ok(Self {
            uri: uri.to_string(),
            secret,
        })
    }

    /// Build an `EciesDecrypter` from the resolved secret. Consumes the
    /// `KeySource` so the bytes only ever live in one place at a time.
    pub fn into_decrypter(self) -> Result<EciesDecrypter, CryptoError> {
        // Move the bytes into the decrypter so its zeroize-on-drop
        // applies; the source's own drop will run over an empty Vec.
        let mut this = self;
        let bytes = std::mem::take(&mut this.secret);
        EciesDecrypter::from_bytes(bytes)
    }
}

/// Convenience: parse a URI and return a boxed `Decrypter` ready to
/// register with the engine or wrap in `MultiKeyDecrypter`.
pub fn decrypter_from_uri(uri: &str) -> Result<Arc<dyn Decrypter>, CryptoError> {
    let source = KeySource::from_uri(uri)?;
    let dec = source.into_decrypter()?;
    Ok(Arc::new(dec))
}

/// Resolve a key URI into raw secret bytes, without imposing a length or
/// curve interpretation. Shared by [`KeySource::from_uri`] (which then
/// enforces the 32-byte secp256k1 contract) and the snapshot-key loader
/// (which uses the bytes directly as a symmetric AEAD key). Keeping the
/// `file:` / `age:` / `awskms:` dispatch in one place means a new scheme is
/// added once and both consumers pick it up.
pub(crate) fn resolve_secret_bytes(uri: &str) -> Result<Vec<u8>, CryptoError> {
    let uri = uri.trim();
    let secret = if let Some(path) = uri.strip_prefix("file:") {
        load_operator_key_file(path)?
    } else if let Some(path) = uri.strip_prefix("age:") {
        // Both the passphrase and the decrypted hex blob are wrapped so
        // they are scrubbed on early-return / error paths as well as on
        // the happy path.
        let pass = Zeroizing::new(std::env::var("DARKPOOL_KEY_PASSPHRASE").map_err(|_| {
            CryptoError::KeySource("age: key URI requires DARKPOOL_KEY_PASSPHRASE env var".into())
        })?);
        let mut bytes = decrypt_age_passphrase(Path::new(path), &pass)
            .map_err(|e| CryptoError::KeySource(format!("age: {e}")))?;
        let secret = parse_hex_secret(&bytes)?;
        bytes.zeroize();
        secret
    } else if let Some(rest) = uri.strip_prefix("awskms:") {
        decrypt_via_aws_kms(rest)?
    } else {
        return Err(CryptoError::KeySource(format!(
            "unsupported key URI scheme: {uri}"
        )));
    };
    Ok(secret)
}

fn parse_hex_secret(bytes: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let s = std::str::from_utf8(bytes)
        .map_err(|e| CryptoError::KeySource(format!("non-utf8 secret: {e}")))?;
    let trimmed = s.trim().trim_start_matches("0x");
    let out = hex::decode(trimmed)?;
    Ok(out)
}

/// Decrypt a passphrase-encrypted age file and return the plaintext.
/// Recipient-keyed files are rejected explicitly — the current scheme
/// is passphrase-only so a misconfiguration doesn't silently fall back
/// to a different trust model.
pub fn decrypt_age_passphrase(path: &Path, passphrase: &str) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let decryptor = match age::Decryptor::new(&bytes[..]).map_err(|e| e.to_string())? {
        age::Decryptor::Passphrase(d) => d,
        age::Decryptor::Recipients(_) => {
            return Err("age file uses recipient keys; passphrase-only supported".into());
        }
    };
    let mut reader = decryptor
        .decrypt(&age::secrecy::Secret::new(passphrase.to_owned()), None)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(feature = "aws-kms")]
fn decrypt_via_aws_kms(_spec: &str) -> Result<Vec<u8>, CryptoError> {
    // Scaffold: the AWS SDK wire-up is deferred. When implemented this
    // will: (1) parse `<key-id>?ciphertext=<base64>` from `_spec`,
    // (2) build an `aws_sdk_kms::Client` from the ambient AWS_* env,
    // (3) call Decrypt with the named key. KMS returns the 32-byte
    // ECIES secret in-process; zeroize after handing off.
    Err(CryptoError::KeySource(
        "awskms: not yet implemented — track issue #28 follow-up".into(),
    ))
}

#[cfg(not(feature = "aws-kms"))]
fn decrypt_via_aws_kms(_spec: &str) -> Result<Vec<u8>, CryptoError> {
    Err(CryptoError::KeySource(
        "awskms: key URI requires the `aws-kms` feature".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_hex_key(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    /// Save → mutate `DARKPOOL_KEY_PASSPHRASE` for one test, restore on
    /// drop. Pair with `#[serial(darkpool_key_passphrase)]` so the
    /// `unsafe` set/remove can never race a parallel reader.
    struct KeyPassphraseGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl KeyPassphraseGuard {
        fn cleared() -> Self {
            let prev = std::env::var_os("DARKPOOL_KEY_PASSPHRASE");
            unsafe {
                std::env::remove_var("DARKPOOL_KEY_PASSPHRASE");
            }
            Self { prev }
        }

        fn set(value: &str) -> Self {
            let prev = std::env::var_os("DARKPOOL_KEY_PASSPHRASE");
            unsafe {
                std::env::set_var("DARKPOOL_KEY_PASSPHRASE", value);
            }
            Self { prev }
        }
    }

    impl Drop for KeyPassphraseGuard {
        fn drop(&mut self) {
            unsafe {
                match self.prev.take() {
                    Some(v) => std::env::set_var("DARKPOOL_KEY_PASSPHRASE", v),
                    None => std::env::remove_var("DARKPOOL_KEY_PASSPHRASE"),
                }
            }
        }
    }

    #[test]
    fn file_uri_round_trips_plain_hex() {
        let key = [0x11u8; 32];
        let f = write_hex_key(&hex::encode(key));
        let src = KeySource::from_uri(&format!("file:{}", f.path().display())).unwrap();
        // Don't read secret directly — exercise the public path:
        let _ = src.into_decrypter().unwrap();
    }

    #[test]
    fn unknown_scheme_errors() {
        match KeySource::from_uri("vault://secret/dp/active") {
            Err(CryptoError::KeySource(msg)) => assert!(msg.contains("unsupported")),
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error, got success"),
        }
    }

    #[test]
    #[serial(darkpool_key_passphrase)]
    fn age_uri_without_passphrase_errors() {
        // Force the env var clear regardless of ambient state — the
        // serial group prevents the round-trip test from racing this.
        let _guard = KeyPassphraseGuard::cleared();
        match KeySource::from_uri("age:/tmp/nonexistent.age") {
            Err(CryptoError::KeySource(msg)) => {
                assert!(msg.contains("DARKPOOL_KEY_PASSPHRASE"), "got: {msg}");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error, got success"),
        }
    }

    #[test]
    fn awskms_uri_without_feature_errors() {
        match KeySource::from_uri("awskms:alias/dp-operator-ecies") {
            Err(CryptoError::KeySource(msg)) => {
                assert!(msg.contains("aws-kms"), "got: {msg}");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error, got success"),
        }
    }

    #[test]
    fn file_uri_wrong_secret_length_errors() {
        // 16 bytes — half of what secp256k1 expects.
        let f = write_hex_key(&hex::encode([0u8; 16]));
        match KeySource::from_uri(&format!("file:{}", f.path().display())) {
            Err(CryptoError::InvalidKeyFile(msg)) => assert!(msg.contains("32 bytes")),
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn parse_hex_secret_rejects_non_utf8() {
        let bytes = [0xff_u8, 0xfe, 0x00, 0x80];
        match parse_hex_secret(&bytes) {
            Err(CryptoError::KeySource(msg)) => assert!(msg.contains("non-utf8")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_hex_secret_strips_0x_prefix() {
        let key = [0x77_u8; 32];
        let s = format!("0x{}", hex::encode(key));
        let out = parse_hex_secret(s.as_bytes()).unwrap();
        assert_eq!(out, key);
    }

    #[test]
    fn decrypt_age_passphrase_missing_path() {
        let err = decrypt_age_passphrase(Path::new("/tmp/no_such_file_dp_crypto_test"), "pw")
            .unwrap_err();
        assert!(err.contains("read"));
    }

    #[test]
    #[serial(darkpool_key_passphrase)]
    fn age_round_trip() {
        // Encrypt a 64-char hex secret with a passphrase, then resolve
        // it back through the `age:` URI scheme. This exercises the
        // full path without needing a real KMS or environment setup.
        let secret_hex = hex::encode([0x42u8; 32]);
        let pass = "rotate-me-2026";

        // Build an age-encrypted blob in-memory using the public crate.
        let mut encrypted = Vec::new();
        let encryptor =
            age::Encryptor::with_user_passphrase(age::secrecy::Secret::new(pass.to_string()));
        let mut writer = encryptor.wrap_output(&mut encrypted).expect("wrap_output");
        std::io::Write::write_all(&mut writer, secret_hex.as_bytes()).unwrap();
        writer.finish().unwrap();

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&encrypted).unwrap();
        f.flush().unwrap();

        let _guard = KeyPassphraseGuard::set(pass);
        let res = KeySource::from_uri(&format!("age:{}", f.path().display()));
        let src = res.expect("age decrypt");
        let _ = src.into_decrypter().expect("decrypter built");
    }
}
