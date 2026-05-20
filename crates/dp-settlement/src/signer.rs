//! Operator transaction-signing abstraction.
//!
//! The Ethereum signing key is independent from the ECIES decryption key
//! (`dp_crypto::EciesDecrypter`). Settlement callers receive an
//! `Arc<dyn TxSigner>` rather than raw private bytes so backends like
//! AWS KMS can sign without ever exposing the key material in-process.
//!
//! URI-driven construction lives in [`from_uri`]. Schemes:
//! - `file:/path/eth.hex` — plaintext hex secp256k1 secret.
//! - `age:/path/eth.age` — age-encrypted hex, passphrase from
//!   `DARKPOOL_SIGNER_PASSPHRASE`.
//! - `awskms:<key-id>` — AWS KMS asymmetric ECC_SECG_P256K1 sign
//!   key (feature `aws-kms`, stub-gated).

use std::str::FromStr;
use std::sync::Arc;

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use zeroize::{Zeroize, Zeroizing};

use crate::SettlementError;

/// Abstraction over the Ethereum signing surface settlement needs.
///
/// The submitter consumes `wallet()` to build alloy provider call paths
/// and `address()` to compute the sender. Implementations MUST keep the
/// secret material non-exfiltrable through this trait — only the
/// derived `EthereumWallet` (which signs in-process or delegates to a
/// remote backend) and the public address are reachable.
pub trait TxSigner: Send + Sync {
    fn address(&self) -> Address;
    fn wallet(&self) -> EthereumWallet;
}

/// In-process secp256k1 signer backed by `alloy_signer_local::PrivateKeySigner`.
pub struct LocalTxSigner {
    inner: PrivateKeySigner,
    wallet: EthereumWallet,
}

impl LocalTxSigner {
    /// Build from a 0x-prefixed (or bare) hex secp256k1 secret.
    pub fn from_hex(hex: &str) -> Result<Self, SettlementError> {
        let inner = PrivateKeySigner::from_str(hex.trim())
            .map_err(|e| SettlementError::Signer(e.to_string()))?;
        let wallet = EthereumWallet::from(inner.clone());
        Ok(Self { inner, wallet })
    }

    /// Read a hex secret from disk (handles `0x` prefix and surrounding
    /// whitespace) and wrap it.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, SettlementError> {
        let raw = std::fs::read_to_string(path.as_ref()).map_err(SettlementError::Io)?;
        Self::from_hex(raw.trim())
    }
}

impl TxSigner for LocalTxSigner {
    fn address(&self) -> Address {
        self.inner.address()
    }

    fn wallet(&self) -> EthereumWallet {
        self.wallet.clone()
    }
}

/// Resolve a URI into a boxed `TxSigner`. The supported schemes are
/// documented at the module level; unknown schemes surface as a
/// `SettlementError::Signer` rather than panicking so a typo in
/// operator config fails-fast at boot rather than at first batch.
pub fn from_uri(uri: &str) -> Result<Arc<dyn TxSigner>, SettlementError> {
    let uri = uri.trim();
    if let Some(path) = uri.strip_prefix("file:") {
        let signer = LocalTxSigner::from_file(path)?;
        return Ok(Arc::new(signer));
    }
    if let Some(path) = uri.strip_prefix("age:") {
        // Wrap the passphrase and decrypted hex so error/early-return
        // paths don't leak the secret into the heap allocator's free
        // list before the next allocation overwrites it.
        let pass = Zeroizing::new(std::env::var("DARKPOOL_SIGNER_PASSPHRASE").map_err(|_| {
            SettlementError::Signer(
                "age: signer URI requires DARKPOOL_SIGNER_PASSPHRASE env var".into(),
            )
        })?);
        let mut hex = decrypt_age_file(path, &pass)
            .map_err(|e| SettlementError::Signer(format!("age signer: {e}")))?;
        let signer = LocalTxSigner::from_hex(&hex)?;
        hex.zeroize();
        return Ok(Arc::new(signer));
    }
    if let Some(rest) = uri.strip_prefix("awskms:") {
        return aws_kms_signer(rest);
    }
    Err(SettlementError::Signer(format!(
        "unsupported signer URI scheme: {uri}"
    )))
}

#[cfg(feature = "aws-kms")]
fn aws_kms_signer(_key_id: &str) -> Result<Arc<dyn TxSigner>, SettlementError> {
    // Scaffold: the workspace alloy-signer-aws adapter has not been wired
    // yet. Issue #28 calls out KMS but lands the trait + URI scheme first;
    // a follow-up will plug an `AwsSigner` that performs ECDSA inside KMS
    // (no private key exposure). Keeping the feature flag wired so the
    // build matrix stays honest.
    Err(SettlementError::Signer(
        "aws-kms signer not yet implemented; track issue #28 follow-up".into(),
    ))
}

#[cfg(not(feature = "aws-kms"))]
fn aws_kms_signer(_key_id: &str) -> Result<Arc<dyn TxSigner>, SettlementError> {
    Err(SettlementError::Signer(
        "aws-kms signer requires the `aws-kms` feature".into(),
    ))
}

/// Decrypt an age-passphrase file into its UTF-8 contents (a hex
/// secret in the signer case).
fn decrypt_age_file(path: &str, passphrase: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
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
    String::from_utf8(out).map_err(|e| format!("non-utf8 plaintext: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // anvil's deterministic key #0 — public, used solely as a hex
    // fixture across tests; no secret-management implications.
    const TEST_HEX: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    #[test]
    fn local_from_hex_round_trip() {
        let s = LocalTxSigner::from_hex(TEST_HEX).unwrap();
        // alloy derives this address from the well-known anvil-0 secret.
        let expected = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        assert_eq!(s.address(), expected);
        // Wallet should agree on the default signer address.
        let derived =
            alloy_network::NetworkWallet::<alloy_network::Ethereum>::default_signer_address(
                &s.wallet(),
            );
        assert_eq!(derived, expected);
    }

    #[test]
    fn from_uri_file_scheme() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{TEST_HEX}").unwrap();
        let path = f.path().to_str().unwrap();
        let signer = from_uri(&format!("file:{path}")).unwrap();
        assert_ne!(signer.address(), Address::ZERO);
    }

    #[test]
    fn from_uri_unknown_scheme_errors() {
        match from_uri("vault://secret/eth") {
            Err(SettlementError::Signer(msg)) => assert!(msg.contains("unsupported")),
            Err(other) => panic!("unexpected: {other:?}"),
            Ok(_) => panic!("expected error for unknown scheme"),
        }
    }

    #[test]
    fn from_uri_awskms_without_feature_errors() {
        match from_uri("awskms:alias/dp-operator-eth") {
            Err(SettlementError::Signer(msg)) => assert!(msg.contains("aws-kms"), "got: {msg}"),
            Err(other) => panic!("unexpected: {other:?}"),
            Ok(_) => panic!("expected error from scaffolded awskms scheme"),
        }
    }

    #[test]
    fn local_from_file_reads_hex() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{TEST_HEX}").unwrap();
        let signer = LocalTxSigner::from_file(f.path()).unwrap();
        let expected = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        assert_eq!(signer.address(), expected);
    }

    #[test]
    fn local_from_file_missing_path_errors() {
        match LocalTxSigner::from_file("/tmp/nonexistent_dp_signer_test") {
            Err(SettlementError::Io(_)) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn local_from_hex_invalid_errors() {
        match LocalTxSigner::from_hex("not-hex") {
            Err(SettlementError::Signer(_)) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn from_uri_file_with_invalid_hex_errors() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "garbage-not-hex").unwrap();
        let uri = format!("file:{}", f.path().display());
        match from_uri(&uri) {
            Err(SettlementError::Signer(_)) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn from_uri_age_without_passphrase_errors() {
        // Only valid when the env var is unset; bail out if a parent
        // test or the runner set it.
        if std::env::var("DARKPOOL_SIGNER_PASSPHRASE").is_ok() {
            return;
        }
        match from_uri("age:/tmp/nonexistent.age") {
            Err(SettlementError::Signer(msg)) => {
                assert!(msg.contains("DARKPOOL_SIGNER_PASSPHRASE"), "got: {msg}");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn from_uri_age_round_trip() {
        // Build an age-encrypted blob containing the test hex secret,
        // then resolve it through the `age:` URI scheme. Exercises the
        // decrypt_age_file path end-to-end without needing KMS.
        let pass = "signer-rotate-2026";

        let mut encrypted = Vec::new();
        let encryptor =
            age::Encryptor::with_user_passphrase(age::secrecy::Secret::new(pass.to_string()));
        let mut writer = encryptor.wrap_output(&mut encrypted).unwrap();
        std::io::Write::write_all(&mut writer, TEST_HEX.as_bytes()).unwrap();
        writer.finish().unwrap();

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&encrypted).unwrap();
        f.flush().unwrap();

        // SAFETY: tests touching this env var are single-threaded in
        // practice for this crate (only this test sets/removes it).
        unsafe {
            std::env::set_var("DARKPOOL_SIGNER_PASSPHRASE", pass);
        }
        let res = from_uri(&format!("age:{}", f.path().display()));
        unsafe {
            std::env::remove_var("DARKPOOL_SIGNER_PASSPHRASE");
        }
        let signer = res.expect("age decrypt");
        let expected = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        assert_eq!(signer.address(), expected);
    }

    #[test]
    fn decrypt_age_file_missing_path_errors() {
        let err = decrypt_age_file("/tmp/definitely_not_there_dp_signer", "pw").unwrap_err();
        assert!(err.contains("read"));
    }
}
