//! Authenticated encryption for engine state snapshots at rest.
//!
//! Snapshots serialize the live order book and pending settlement matches,
//! both of which carry cleartext `trader` / `price` / `size`. Writing them as
//! bare bincode leaks resting-order data to anyone with read access to the
//! `SnapshotStore` (issue #203). [`SnapshotCipher`] seals the serialized blob
//! under a dedicated symmetric key with XChaCha20-Poly1305, so the store holds
//! only ciphertext plus an authentication tag.
//!
//! The key is *dedicated* — distinct from the operator ECIES identity — so a
//! snapshot-key compromise cannot decrypt live order ciphertext, and rotating
//! the operator key never strands snapshot recovery. It is resolved through the
//! same [`crate::KeySource`] URI machinery (`file:` / `age:` / `awskms:`).
//!
//! XChaCha20-Poly1305 takes a 192-bit nonce, generated randomly per seal. That
//! width makes accidental nonce reuse statistically impossible without a
//! counter, which suits independently-encrypted snapshot blobs.

use chacha20poly1305::aead::rand_core::{OsRng, RngCore};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::{Zeroize, Zeroizing};

use crate::key_source::resolve_secret_bytes;
use crate::CryptoError;

/// XChaCha20-Poly1305 nonce width (bytes). Stored as the prefix of every
/// sealed blob produced by [`SnapshotCipher::seal`].
pub const SNAPSHOT_NONCE_LEN: usize = 24;

const SNAPSHOT_KEY_LEN: usize = 32;

/// A symmetric cipher for sealing/opening snapshot blobs. Holds the key inside
/// an `XChaCha20Poly1305` instance, which zeroizes its key material on drop.
pub struct SnapshotCipher {
    cipher: XChaCha20Poly1305,
}

impl SnapshotCipher {
    /// Build from raw 32-byte key material. Any 32 bytes are a valid symmetric
    /// key (no curve constraint, unlike the operator ECIES secret).
    pub fn from_bytes(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != SNAPSHOT_KEY_LEN {
            return Err(CryptoError::InvalidKeyFile(format!(
                "snapshot key must be {SNAPSHOT_KEY_LEN} bytes, got {}",
                key.len()
            )));
        }
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| CryptoError::InvalidKeyFile(format!("snapshot key: {e}")))?;
        Ok(Self { cipher })
    }

    /// Resolve a dedicated snapshot key from a [`crate::KeySource`] URI
    /// (`file:` / `age:` / `awskms:`) and build a cipher. The resolved bytes
    /// are zeroized before returning regardless of outcome.
    pub fn from_key_uri(uri: &str) -> Result<Self, CryptoError> {
        let mut bytes = resolve_secret_bytes(uri)?;
        let result = Self::from_bytes(&bytes);
        bytes.zeroize();
        result
    }

    /// Generate a process-lifetime random key. Used only for the in-memory
    /// (non-durable) snapshot store: those snapshots never survive a restart
    /// anyway, so an ephemeral key has no recovery downside while still keeping
    /// at-rest bytes encrypted.
    pub fn generate_ephemeral() -> Self {
        let mut key = Zeroizing::new([0u8; SNAPSHOT_KEY_LEN]);
        OsRng.fill_bytes(key.as_mut_slice());
        // `from_bytes` only fails on a length mismatch, impossible for a fixed
        // 32-byte array.
        Self::from_bytes(key.as_slice()).expect("32-byte key is always valid")
    }

    /// Seal `plaintext`, binding `aad` (the envelope header identity) into the
    /// authentication tag. Returns `nonce || ciphertext+tag`. A fresh random
    /// nonce is drawn per call.
    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce_bytes = [0u8; SNAPSHOT_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from(nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad })
            .map_err(|e| CryptoError::DecryptionFailed(format!("snapshot seal: {e}")))?;
        let mut out = Vec::with_capacity(SNAPSHOT_NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Open a `nonce || ciphertext+tag` blob produced by [`Self::seal`],
    /// verifying `aad`. Any tampering — ciphertext, tag, nonce, or a mismatched
    /// `aad` — fails with [`CryptoError::DecryptionFailed`].
    pub fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if sealed.len() < SNAPSHOT_NONCE_LEN {
            return Err(CryptoError::DecryptionFailed(format!(
                "snapshot blob too short: {} bytes (need at least {SNAPSHOT_NONCE_LEN})",
                sealed.len()
            )));
        }
        let (nonce_bytes, ciphertext) = sealed.split_at(SNAPSHOT_NONCE_LEN);
        let nonce_arr: [u8; SNAPSHOT_NONCE_LEN] = nonce_bytes
            .try_into()
            .expect("split_at(SNAPSHOT_NONCE_LEN) yields exactly that many bytes");
        let nonce = XNonce::from(nonce_arr);
        self.cipher
            .decrypt(&nonce, Payload { msg: ciphertext, aad })
            .map_err(|e| CryptoError::DecryptionFailed(format!("snapshot open: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn cipher() -> SnapshotCipher {
        SnapshotCipher::from_bytes(&[0x42u8; 32]).unwrap()
    }

    #[test]
    fn seal_open_round_trip() {
        let c = cipher();
        let plaintext = b"resting order: trader=0xabc price=1500 size=3";
        let aad = b"DPS2\x00\x00\x00\x02seq=7";
        let sealed = c.seal(plaintext, aad).unwrap();
        // Ciphertext must not echo the plaintext.
        assert!(!sealed.windows(plaintext.len()).any(|w| w == plaintext));
        let opened = c.open(&sealed, aad).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn nonce_is_random_per_seal() {
        let c = cipher();
        let a = c.seal(b"same", b"aad").unwrap();
        let b = c.seal(b"same", b"aad").unwrap();
        // Different nonces ⇒ different sealed bytes for identical input.
        assert_ne!(a, b);
    }

    #[test]
    fn open_rejects_tampered_ciphertext() {
        let c = cipher();
        let mut sealed = c.seal(b"secret", b"aad").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(matches!(
            c.open(&sealed, b"aad"),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn open_rejects_wrong_key() {
        let sealed = cipher().seal(b"secret", b"aad").unwrap();
        let other = SnapshotCipher::from_bytes(&[0x99u8; 32]).unwrap();
        assert!(matches!(
            other.open(&sealed, b"aad"),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn open_rejects_wrong_aad() {
        let c = cipher();
        let sealed = c.seal(b"secret", b"aad-one").unwrap();
        // A mismatched AAD (e.g. a relabeled seq) must fail the tag check.
        assert!(matches!(
            c.open(&sealed, b"aad-two"),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn open_rejects_truncated_blob() {
        let c = cipher();
        assert!(matches!(
            c.open(&[0u8; 8], b"aad"),
            Err(CryptoError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert!(matches!(
            SnapshotCipher::from_bytes(&[0u8; 16]),
            Err(CryptoError::InvalidKeyFile(_))
        ));
    }

    #[test]
    fn from_key_uri_file_round_trips() {
        let key = [0xCDu8; 32];
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(hex::encode(key).as_bytes()).unwrap();
        f.flush().unwrap();

        let c = SnapshotCipher::from_key_uri(&format!("file:{}", f.path().display())).unwrap();
        let sealed = c.seal(b"hello", b"aad").unwrap();
        // A cipher built straight from the same bytes opens it.
        let direct = SnapshotCipher::from_bytes(&key).unwrap();
        assert_eq!(direct.open(&sealed, b"aad").unwrap(), b"hello");
    }

    #[test]
    fn from_key_uri_rejects_short_file_key() {
        // `file:` keys must be 32-byte hex; a 16-byte key is rejected upstream.
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(hex::encode([0u8; 16]).as_bytes()).unwrap();
        f.flush().unwrap();
        assert!(SnapshotCipher::from_key_uri(&format!("file:{}", f.path().display())).is_err());
    }

    #[test]
    fn from_key_uri_rejects_unknown_scheme() {
        assert!(matches!(
            SnapshotCipher::from_key_uri("vault://snapshot"),
            Err(CryptoError::KeySource(_))
        ));
    }

    #[test]
    fn generate_ephemeral_round_trips_but_differs_per_instance() {
        let a = SnapshotCipher::generate_ephemeral();
        let b = SnapshotCipher::generate_ephemeral();
        let sealed = a.seal(b"payload", b"aad").unwrap();
        assert_eq!(a.open(&sealed, b"aad").unwrap(), b"payload");
        // An independent ephemeral key cannot open it.
        assert!(b.open(&sealed, b"aad").is_err());
    }
}
