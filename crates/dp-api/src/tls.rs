//! TLS / mTLS material loader shared by the gRPC and REST listeners.
//!
//! Both transports terminate TLS in-process — there is no sidecar — so a
//! single loader keeps the cert handling logic in one place and ensures
//! the two listeners can never drift on parse rules or trust roots.
//!
//! The public surface is three thin helpers driven by [`TlsMode`]:
//!
//! - [`tonic_server_tls`] builds a tonic [`ServerTlsConfig`] for the
//!   gRPC listener.
//! - [`axum_rustls_config`] builds an axum-server [`RustlsConfig`] for
//!   the REST listener. The mTLS branch returns a config constructed
//!   from a full [`rustls::ServerConfig`] so [`reload_axum_rustls`]
//!   can swap in new material without restarting the listener.
//! - [`reload_axum_rustls`] swaps the REST cert in-place on SIGHUP.
//!   The gRPC side has no equivalent (tonic 0.12 exposes no reload
//!   API) — callers must restart the process to rotate the gRPC cert.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tonic::transport::{Certificate, Identity, ServerTlsConfig};

use crate::config::TlsMode;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("tls io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("malformed pem at {path}: {msg}")]
    MalformedPem { path: String, msg: String },

    #[error("malformed private key at {path}: no PEM-encoded key block found")]
    MalformedKey { path: String },

    #[error("malformed client CA at {path}: {msg}")]
    MalformedCa { path: String, msg: String },

    #[error("rustls config error: {0}")]
    Rustls(String),
}

/// Build a tonic [`ServerTlsConfig`] for the requested posture.
/// Returns `Ok(None)` for plaintext.
pub fn tonic_server_tls(mode: &TlsMode) -> Result<Option<ServerTlsConfig>, TlsError> {
    match mode {
        TlsMode::Plaintext => Ok(None),
        TlsMode::Tls { cert, key } => {
            let cert_pem = read_file(cert)?;
            let key_pem = read_file(key)?;
            // tonic 0.12's `Identity::from_pem` is an infallible builder;
            // without an explicit sanity-parse, malformed cert/key would
            // surface at the first TLS handshake instead of at boot.
            let _ = load_cert_chain_from_bytes(&cert_pem, cert)?;
            let _ = load_private_key_from_bytes(&key_pem, key)?;
            let identity = Identity::from_pem(&cert_pem, &key_pem);
            Ok(Some(ServerTlsConfig::new().identity(identity)))
        }
        TlsMode::Mtls {
            cert,
            key,
            client_ca,
        } => {
            let cert_pem = read_file(cert)?;
            let key_pem = read_file(key)?;
            let ca_pem = read_file(client_ca)?;
            // tonic 0.12's `Identity::from_pem` / `Certificate::from_pem`
            // are infallible builders, so we sanity-parse here. Otherwise
            // a malformed bundle would present as silent "no client
            // accepted" handshakes instead of failing at boot.
            let _ = load_cert_chain_from_bytes(&cert_pem, cert)?;
            let _ = load_private_key_from_bytes(&key_pem, key)?;
            let _ = load_ca_bundle_from_bytes(&ca_pem, client_ca)?;
            let identity = Identity::from_pem(&cert_pem, &key_pem);
            Ok(Some(
                ServerTlsConfig::new()
                    .identity(identity)
                    .client_ca_root(Certificate::from_pem(ca_pem)),
            ))
        }
    }
}

/// Build an [`RustlsConfig`] for the REST listener. Returns `Ok(None)`
/// for plaintext.
pub async fn axum_rustls_config(mode: &TlsMode) -> Result<Option<RustlsConfig>, TlsError> {
    install_crypto_provider();
    match mode {
        TlsMode::Plaintext => Ok(None),
        TlsMode::Tls { cert, key } => {
            let cfg = RustlsConfig::from_pem_file(cert, key)
                .await
                .map_err(|e| TlsError::Rustls(e.to_string()))?;
            Ok(Some(cfg))
        }
        TlsMode::Mtls {
            cert,
            key,
            client_ca,
        } => {
            let server_cfg = build_mtls_server_config(cert, key, client_ca)?;
            Ok(Some(RustlsConfig::from_config(Arc::new(server_cfg))))
        }
    }
}

/// Reload the REST listener's certificate material in-place. Open
/// connections continue with the old material; new TLS handshakes after
/// this call use the reloaded cert.
pub async fn reload_axum_rustls(cfg: &RustlsConfig, mode: &TlsMode) -> Result<(), TlsError> {
    install_crypto_provider();
    match mode {
        TlsMode::Plaintext => Err(TlsError::Rustls(
            "cannot hot-reload TLS for a plaintext listener — restart the process".into(),
        )),
        TlsMode::Tls { cert, key } => cfg
            .reload_from_pem_file(cert, key)
            .await
            .map_err(|e| TlsError::Rustls(e.to_string())),
        TlsMode::Mtls {
            cert,
            key,
            client_ca,
        } => {
            // mTLS requires a full ServerConfig (the verifier is part of
            // it), so go through reload_from_config rather than the PEM
            // helper, which only knows about cert+key.
            let server_cfg = build_mtls_server_config(cert, key, client_ca)?;
            cfg.reload_from_config(Arc::new(server_cfg));
            Ok(())
        }
    }
}

fn build_mtls_server_config(
    cert: &Path,
    key: &Path,
    client_ca: &Path,
) -> Result<ServerConfig, TlsError> {
    let cert_chain = load_cert_chain(cert)?;
    let private_key = load_private_key(key)?;
    let roots = load_ca_bundle(client_ca)?;
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| TlsError::Rustls(e.to_string()))?;
    ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| TlsError::Rustls(e.to_string()))
}

fn read_file(path: &Path) -> Result<Vec<u8>, TlsError> {
    fs::read(path).map_err(|source| TlsError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn load_cert_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let pem = read_file(path)?;
    load_cert_chain_from_bytes(&pem, path)
}

fn load_cert_chain_from_bytes(
    pem: &[u8],
    path_for_error: &Path,
) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let mut reader = pem;
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::MalformedPem {
            path: path_for_error.display().to_string(),
            msg: e.to_string(),
        })
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    let pem = read_file(path)?;
    load_private_key_from_bytes(&pem, path)
}

fn load_private_key_from_bytes(
    pem: &[u8],
    path_for_error: &Path,
) -> Result<PrivateKeyDer<'static>, TlsError> {
    let mut reader = pem;
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| TlsError::MalformedPem {
            path: path_for_error.display().to_string(),
            msg: e.to_string(),
        })?
        .ok_or_else(|| TlsError::MalformedKey {
            path: path_for_error.display().to_string(),
        })
}

fn load_ca_bundle(path: &Path) -> Result<RootCertStore, TlsError> {
    let pem = read_file(path)?;
    load_ca_bundle_from_bytes(&pem, path)
}

fn load_ca_bundle_from_bytes(
    pem: &[u8],
    path_for_error: &Path,
) -> Result<RootCertStore, TlsError> {
    let mut reader = pem;
    let mut roots = RootCertStore::empty();
    let mut count = 0usize;
    for cert in rustls_pemfile::certs(&mut reader) {
        let cert = cert.map_err(|e| TlsError::MalformedCa {
            path: path_for_error.display().to_string(),
            msg: e.to_string(),
        })?;
        roots.add(cert).map_err(|e| TlsError::MalformedCa {
            path: path_for_error.display().to_string(),
            msg: e.to_string(),
        })?;
        count += 1;
    }
    if count == 0 {
        return Err(TlsError::MalformedCa {
            path: path_for_error.display().to_string(),
            msg: "no CA certificates found in bundle".into(),
        });
    }
    Ok(roots)
}

// rustls 0.23 requires exactly one default CryptoProvider per process.
// axum-server and tonic both initialize their own clients on first use;
// installing ring up-front guarantees both paths agree on the provider.
fn install_crypto_provider() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn plaintext_returns_none_for_both_listeners() {
        let mode = TlsMode::Plaintext;
        assert!(tonic_server_tls(&mode).unwrap().is_none());
        // axum_rustls_config is async; reuse current_thread runtime so
        // this stays inside the unit-test module without #[tokio::test]
        // bringing the full multi-thread runtime in.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert!(rt.block_on(axum_rustls_config(&mode)).unwrap().is_none());
    }

    #[test]
    fn missing_cert_surfaces_io_error_with_path() {
        let mode = TlsMode::Tls {
            cert: PathBuf::from("/definitely/missing/cert.pem"),
            key: PathBuf::from("/definitely/missing/key.pem"),
        };
        let err = tonic_server_tls(&mode).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/definitely/missing/cert.pem"), "msg: {msg}");
    }

    #[test]
    fn load_ca_bundle_rejects_empty_pem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.pem");
        std::fs::write(&path, b"not a real pem block\n").unwrap();
        let err = load_ca_bundle(&path).unwrap_err();
        assert!(matches!(err, TlsError::MalformedCa { .. }));
    }

    #[test]
    fn load_private_key_rejects_pem_without_key_block() {
        // rustls_pemfile::private_key returns `Ok(None)` when the file
        // parses but contains no recognized PRIVATE KEY block. We map
        // that to MalformedKey so operators see the real cause instead
        // of a misleading "loaded ok but key missing" silent path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nokey.pem");
        std::fs::write(
            &path,
            b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let err = load_private_key(&path).unwrap_err();
        assert!(matches!(err, TlsError::MalformedKey { .. }), "got: {err:?}");
    }

    // Each loader has two failure modes: PEM framing parse error
    // (rustls_pemfile iterator yields Err) and structural-content error
    // (block parses but bytes aren't valid X.509 / PKCS8). The fixtures
    // below trigger the iterator-Err path so the `map_err(|e|
    // TlsError::Malformed* { msg: e.to_string(), .. })` closures don't
    // silently rot if a future rustls_pemfile bump changes the wording.
    const MALFORMED_CERT_PEM: &[u8] =
        b"-----BEGIN CERTIFICATE-----\n!!!invalid base64!!!\n-----END CERTIFICATE-----\n";
    const MALFORMED_KEY_PEM: &[u8] =
        b"-----BEGIN PRIVATE KEY-----\n!!!invalid base64!!!\n-----END PRIVATE KEY-----\n";

    #[test]
    fn load_cert_chain_surfaces_pem_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badcert.pem");
        std::fs::write(&path, MALFORMED_CERT_PEM).unwrap();
        let err = load_cert_chain(&path).unwrap_err();
        assert!(matches!(err, TlsError::MalformedPem { .. }), "got: {err:?}");
    }

    #[test]
    fn load_private_key_surfaces_pem_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badkey.pem");
        std::fs::write(&path, MALFORMED_KEY_PEM).unwrap();
        let err = load_private_key(&path).unwrap_err();
        assert!(matches!(err, TlsError::MalformedPem { .. }), "got: {err:?}");
    }

    #[test]
    fn load_ca_bundle_surfaces_pem_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badca.pem");
        std::fs::write(&path, MALFORMED_CERT_PEM).unwrap();
        let err = load_ca_bundle(&path).unwrap_err();
        assert!(matches!(err, TlsError::MalformedCa { .. }), "got: {err:?}");
    }
}
