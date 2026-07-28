//! End-to-end TLS / mTLS coverage for the gRPC and REST listeners.
//!
//! Each test mints an ephemeral CA + server cert (+ client cert where
//! the case needs one) via rcgen, wires the real transport stack the
//! same way `darkpool-server` boots it in `main.rs`, then drives it
//! with a real tonic or reqwest client. No fixtures are committed —
//! ~5 ms per mint keeps these tests cheap and free of rot risk.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use dp_api::config::TlsMode;
use dp_api::handler::ApiHandler;
use dp_api::pb::dark_pool_service_client::DarkPoolServiceClient;
use dp_api::pb::dark_pool_service_server::DarkPoolServiceServer;
use dp_api::pb::ListPairsRequest;
use dp_api::rest;
use dp_api::tls;
use dp_api::validation::PLACE_ORDER_BODY_LIMIT;
use dp_engine::Engine;
use dp_event::MemStore;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity, Server};

// rcgen 0.13 produces both PEMs from the same KeyPair / Certificate
// types, so we keep them paired in the bundle.
struct CertMaterial {
    pem: String,
    key_pem: String,
}

struct CertBundle {
    _dir: TempDir,
    ca_path: std::path::PathBuf,
    server_cert_path: std::path::PathBuf,
    server_key_path: std::path::PathBuf,
    client: CertMaterial,
    ca_pem: String,
}

fn mint_bundle() -> CertBundle {
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};

    let ca_kp = KeyPair::generate().expect("ca keypair");
    let mut ca_params = CertificateParams::new(vec!["DP Test Root CA".into()]).expect("ca params");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self_signed");

    let server_kp = KeyPair::generate().expect("server keypair");
    let server_params =
        CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).expect("srv params");
    let server_cert = server_params
        .signed_by(&server_kp, &ca_cert, &ca_kp)
        .expect("server signed_by");

    let client_kp = KeyPair::generate().expect("client keypair");
    let client_params =
        CertificateParams::new(vec!["dp-test-client".into()]).expect("client params");
    let client_cert = client_params
        .signed_by(&client_kp, &ca_cert, &ca_kp)
        .expect("client signed_by");

    let dir = tempfile::tempdir().expect("tempdir");
    let ca_path = dir.path().join("ca.pem");
    let server_cert_path = dir.path().join("server.pem");
    let server_key_path = dir.path().join("server.key");
    let ca_pem = ca_cert.pem();
    std::fs::write(&ca_path, &ca_pem).unwrap();
    std::fs::write(&server_cert_path, server_cert.pem()).unwrap();
    std::fs::write(&server_key_path, server_kp.serialize_pem()).unwrap();

    CertBundle {
        _dir: dir,
        ca_path,
        server_cert_path,
        server_key_path,
        client: CertMaterial {
            pem: client_cert.pem(),
            key_pem: client_kp.serialize_pem(),
        },
        ca_pem,
    }
}

fn ensure_crypto_provider() {
    // rustls 0.23 panics in tests if multiple crates pull in different
    // providers and none has been installed as the default. Calling
    // install_default() repeatedly is safe — the second-and-onward
    // attempts return `Err(...)`, which we deliberately ignore.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn fresh_handler() -> ApiHandler {
    let store = Arc::new(MemStore::new());
    let engine = Engine::new(store, Duration::from_secs(1));
    engine.register_pair_without_event("ETH/USDC".into(), dp_engine::PairConfig::default());
    ApiHandler::new(engine)
}

async fn bind_local() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    (listener, addr)
}

struct GrpcServer {
    addr: SocketAddr,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl GrpcServer {
    async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

async fn start_grpc(mode: &TlsMode) -> GrpcServer {
    ensure_crypto_provider();
    let handler = fresh_handler();
    let (listener, addr) = bind_local().await;
    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let tls_cfg = tls::tonic_server_tls(mode).expect("tonic tls config");
    let task = tokio::spawn(async move {
        let mut b = Server::builder();
        if let Some(tls) = tls_cfg {
            b = b.tls_config(tls).expect("tls_config");
        }
        b.add_service(
            DarkPoolServiceServer::new(handler).max_decoding_message_size(PLACE_ORDER_BODY_LIMIT),
        )
        .serve_with_incoming_shutdown(incoming, async move { server_cancel.cancelled().await })
        .await
        .expect("grpc server");
    });
    GrpcServer { addr, cancel, task }
}

struct RestServer {
    addr: SocketAddr,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    tls_cfg: Option<RustlsConfig>,
}

impl RestServer {
    async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

async fn start_rest(mode: &TlsMode) -> RestServer {
    ensure_crypto_provider();
    let handler = Arc::new(fresh_handler());
    let app = rest::router(handler);
    let (listener, addr) = bind_local().await;
    drop(listener); // axum_server::bind takes the addr, re-binds itself.
    let cancel = CancellationToken::new();
    let server_handle = axum_server::Handle::new();
    let shutdown_handle = server_handle.clone();
    let shutdown_cancel = cancel.clone();
    let tls_cfg = tls::axum_rustls_config(mode)
        .await
        .expect("rest tls config");
    let tls_for_test = tls_cfg.clone();
    let make_svc = app.into_make_service_with_connect_info::<SocketAddr>();
    let task = tokio::spawn(async move {
        tokio::spawn(async move {
            shutdown_cancel.cancelled().await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(5)));
        });
        let result = match tls_cfg {
            Some(cfg) => {
                axum_server::bind_rustls(addr, cfg)
                    .handle(server_handle)
                    .serve(make_svc)
                    .await
            }
            None => {
                axum_server::bind(addr)
                    .handle(server_handle)
                    .serve(make_svc)
                    .await
            }
        };
        if let Err(e) = result {
            eprintln!("rest server exit: {e}");
        }
    });
    RestServer {
        addr,
        cancel,
        task,
        tls_cfg: tls_for_test,
    }
}

async fn grpc_client(addr: SocketAddr, ca_pem: &str) -> DarkPoolServiceClient<Channel> {
    let endpoint = format!("https://localhost:{}", addr.port());
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .domain_name("localhost");
    let mut last_err = None;
    for _ in 0..50 {
        let ch = Channel::from_shared(endpoint.clone())
            .unwrap()
            .tls_config(tls.clone())
            .unwrap()
            .connect()
            .await;
        match ch {
            Ok(c) => return DarkPoolServiceClient::new(c),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    panic!("grpc client connect failed: {:?}", last_err);
}

async fn grpc_client_mtls(
    addr: SocketAddr,
    ca_pem: &str,
    client_cert_pem: &str,
    client_key_pem: &str,
) -> DarkPoolServiceClient<Channel> {
    let endpoint = format!("https://localhost:{}", addr.port());
    let identity = Identity::from_pem(client_cert_pem, client_key_pem);
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .identity(identity)
        .domain_name("localhost");
    let mut last_err = None;
    for _ in 0..50 {
        let ch = Channel::from_shared(endpoint.clone())
            .unwrap()
            .tls_config(tls.clone())
            .unwrap()
            .connect()
            .await;
        match ch {
            Ok(c) => return DarkPoolServiceClient::new(c),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    panic!("grpc mtls client connect failed: {:?}", last_err);
}

fn reqwest_client(ca_pem: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(ca_pem.as_bytes()).unwrap())
        .build()
        .unwrap()
}

fn reqwest_client_mtls(
    ca_pem: &str,
    client_cert_pem: &str,
    client_key_pem: &str,
) -> reqwest::Client {
    let mut bundle = Vec::new();
    bundle.extend_from_slice(client_cert_pem.as_bytes());
    bundle.extend_from_slice(client_key_pem.as_bytes());
    let identity = reqwest::Identity::from_pem(&bundle).unwrap();
    reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(ca_pem.as_bytes()).unwrap())
        .identity(identity)
        .build()
        .unwrap()
}

async fn wait_rest_ready(client: &reqwest::Client, url: &str) {
    for _ in 0..100 {
        if client.get(url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("rest server never became ready at {url}");
}

#[tokio::test]
async fn grpc_serves_over_tls() {
    let bundle = mint_bundle();
    let mode = TlsMode::Tls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
    };
    let server = start_grpc(&mode).await;
    let mut client = grpc_client(server.addr, &bundle.ca_pem).await;

    let resp = client
        .list_pairs(ListPairsRequest {})
        .await
        .expect("list_pairs over TLS")
        .into_inner();
    assert!(resp.pairs.iter().any(|p| p.pair == "ETH/USDC"));

    server.shutdown().await;
}

#[tokio::test]
async fn grpc_rejects_plain_client_when_tls_on() {
    let bundle = mint_bundle();
    let mode = TlsMode::Tls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
    };
    let server = start_grpc(&mode).await;

    // Plain h2 client (no TLS) against a TLS listener: connect either
    // fails outright or the first RPC fails — either is an acceptable
    // "rejected" outcome. We bound the wait so a hung handshake can't
    // freeze the test.
    let endpoint = format!("http://{}", server.addr);
    let channel = Channel::from_shared(endpoint).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(2), async {
        match channel.connect().await {
            Err(_) => true, // connect refused
            Ok(ch) => {
                let mut client = DarkPoolServiceClient::new(ch);
                client.list_pairs(ListPairsRequest {}).await.is_err()
            }
        }
    })
    .await
    .unwrap_or(true);
    assert!(
        outcome,
        "plaintext client must not succeed against TLS server"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn grpc_mtls_accepts_authorized_client() {
    let bundle = mint_bundle();
    let mode = TlsMode::Mtls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
        client_ca: bundle.ca_path.clone(),
    };
    let server = start_grpc(&mode).await;
    let mut client = grpc_client_mtls(
        server.addr,
        &bundle.ca_pem,
        &bundle.client.pem,
        &bundle.client.key_pem,
    )
    .await;

    let resp = client
        .list_pairs(ListPairsRequest {})
        .await
        .expect("mTLS list_pairs")
        .into_inner();
    assert!(resp.pairs.iter().any(|p| p.pair == "ETH/USDC"));

    server.shutdown().await;
}

#[tokio::test]
async fn grpc_mtls_rejects_missing_client_cert() {
    let bundle = mint_bundle();
    let mode = TlsMode::Mtls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
        client_ca: bundle.ca_path.clone(),
    };
    let server = start_grpc(&mode).await;

    // CA-trusting client *without* an identity: handshake must fail.
    let endpoint = format!("https://localhost:{}", server.addr.port());
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(&bundle.ca_pem))
        .domain_name("localhost");

    let outcome = tokio::time::timeout(Duration::from_secs(3), async {
        // Try a few times — connect can race the listener.
        for _ in 0..20 {
            let r = Channel::from_shared(endpoint.clone())
                .unwrap()
                .tls_config(tls.clone())
                .unwrap()
                .connect()
                .await;
            match r {
                Err(_) => return true,
                Ok(ch) => {
                    let mut c = DarkPoolServiceClient::new(ch);
                    if c.list_pairs(ListPairsRequest {}).await.is_err() {
                        return true;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    })
    .await
    .unwrap_or(true);
    assert!(outcome, "mTLS server must reject clients without certs");

    server.shutdown().await;
}

#[tokio::test]
async fn rest_serves_over_tls() {
    let bundle = mint_bundle();
    let mode = TlsMode::Tls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
    };
    let server = start_rest(&mode).await;
    let client = reqwest_client(&bundle.ca_pem);
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());
    wait_rest_ready(&client, &url).await;
    let resp = client.get(&url).send().await.expect("rest GET");
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);
    server.shutdown().await;
}

#[tokio::test]
async fn rest_mtls_accepts_authorized_client() {
    let bundle = mint_bundle();
    let mode = TlsMode::Mtls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
        client_ca: bundle.ca_path.clone(),
    };
    let server = start_rest(&mode).await;
    let client = reqwest_client_mtls(&bundle.ca_pem, &bundle.client.pem, &bundle.client.key_pem);
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());
    wait_rest_ready(&client, &url).await;
    let resp = client.get(&url).send().await.expect("rest mTLS GET");
    assert_eq!(resp.status(), 200);
    server.shutdown().await;
}

#[tokio::test]
async fn rest_mtls_rejects_missing_client_cert() {
    let bundle = mint_bundle();
    let mode = TlsMode::Mtls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
        client_ca: bundle.ca_path.clone(),
    };
    let server = start_rest(&mode).await;
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());

    // First wait for the listener to be ready via the mTLS-authorized
    // client. This separates "server not bound yet" from "TLS rejected
    // me" — otherwise a connect-refused early on would look identical
    // to a TLS handshake failure and the assertion would lie.
    let authed = reqwest_client_mtls(&bundle.ca_pem, &bundle.client.pem, &bundle.client.key_pem);
    wait_rest_ready(&authed, &url).await;

    // No client identity → the TLS handshake must fail.
    let anon = reqwest_client(&bundle.ca_pem);
    let outcome = anon.get(&url).send().await;
    assert!(
        outcome.is_err(),
        "mTLS server must reject reqwest client without identity, got status {:?}",
        outcome.ok().map(|r| r.status())
    );
    server.shutdown().await;
}

#[tokio::test]
async fn rest_reload_swaps_cert_material() {
    // Boots REST with cert-A, swaps to cert-B via the same reload path
    // SIGHUP triggers in production, then verifies the new cert is
    // served on subsequent handshakes. Mirrors the rotation runbook
    // step without sending an actual signal.
    let bundle_a = mint_bundle();
    let bundle_b = mint_bundle();

    let mode_a = TlsMode::Tls {
        cert: bundle_a.server_cert_path.clone(),
        key: bundle_a.server_key_path.clone(),
    };
    let mode_b = TlsMode::Tls {
        cert: bundle_b.server_cert_path.clone(),
        key: bundle_b.server_key_path.clone(),
    };

    let server = start_rest(&mode_a).await;
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());

    let client_a = reqwest_client(&bundle_a.ca_pem);
    wait_rest_ready(&client_a, &url).await;
    assert_eq!(client_a.get(&url).send().await.unwrap().status(), 200);

    // Cert-B is signed by a different CA, so a CA-A client should now
    // fail post-reload, and a CA-B client should succeed. This proves
    // the swap actually replaced the chain rather than masking the
    // assertion behind a still-valid old cert.
    let tls_cfg_handle = server.tls_cfg.clone().expect("tls config present");
    tls::reload_axum_rustls(&tls_cfg_handle, &mode_b)
        .await
        .expect("reload");

    // Reqwest reuses kept-alive connections; force a fresh client so we
    // exercise a new TLS handshake against the reloaded cert.
    let client_b = reqwest_client(&bundle_b.ca_pem);
    let resp = client_b.get(&url).send().await.expect("post-reload GET");
    assert_eq!(resp.status(), 200);

    let client_a_after = reqwest_client(&bundle_a.ca_pem);
    let fail = client_a_after.get(&url).send().await;
    assert!(
        fail.is_err(),
        "old-CA client must fail after rotation, got {:?}",
        fail.ok().map(|r| r.status())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn rest_mtls_reload_swaps_cert_material() {
    // Happy-path twin to rest_reload_swaps_cert_material on the mTLS
    // arm. Exercises build_mtls_server_config + cfg.reload_from_config,
    // the path SIGHUP takes when the operator runs mTLS in production.
    let bundle_a = mint_bundle();
    let bundle_b = mint_bundle();
    let mode_a = TlsMode::Mtls {
        cert: bundle_a.server_cert_path.clone(),
        key: bundle_a.server_key_path.clone(),
        client_ca: bundle_a.ca_path.clone(),
    };
    let mode_b = TlsMode::Mtls {
        cert: bundle_b.server_cert_path.clone(),
        key: bundle_b.server_key_path.clone(),
        client_ca: bundle_b.ca_path.clone(),
    };

    let server = start_rest(&mode_a).await;
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());

    let client_a = reqwest_client_mtls(
        &bundle_a.ca_pem,
        &bundle_a.client.pem,
        &bundle_a.client.key_pem,
    );
    wait_rest_ready(&client_a, &url).await;
    assert_eq!(client_a.get(&url).send().await.unwrap().status(), 200);

    let tls_cfg_handle = server.tls_cfg.clone().expect("tls config present");
    tls::reload_axum_rustls(&tls_cfg_handle, &mode_b)
        .await
        .expect("mTLS reload");

    let client_b = reqwest_client_mtls(
        &bundle_b.ca_pem,
        &bundle_b.client.pem,
        &bundle_b.client.key_pem,
    );
    let resp = client_b
        .get(&url)
        .send()
        .await
        .expect("post-reload mTLS GET");
    assert_eq!(resp.status(), 200);

    server.shutdown().await;
}

#[tokio::test]
async fn rest_reload_to_plaintext_errors() {
    // Downgrading a TLS listener to plaintext via SIGHUP is intentionally
    // refused — the only safe path is process restart. The function must
    // surface this as Err so the SIGHUP handler logs and keeps the live
    // cert serving rather than silently discarding TLS.
    let bundle = mint_bundle();
    let mode = TlsMode::Tls {
        cert: bundle.server_cert_path.clone(),
        key: bundle.server_key_path.clone(),
    };
    let server = start_rest(&mode).await;
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());
    let client = reqwest_client(&bundle.ca_pem);
    wait_rest_ready(&client, &url).await;

    let tls_cfg_handle = server.tls_cfg.clone().expect("tls config present");
    let outcome = tls::reload_axum_rustls(&tls_cfg_handle, &TlsMode::Plaintext).await;
    assert!(
        outcome.is_err(),
        "plaintext reload must Err, got {outcome:?}"
    );

    // Old TLS cert still serving.
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);

    server.shutdown().await;
}

#[tokio::test]
async fn rest_reload_failure_keeps_old_cert() {
    // Failure-soft contract: a SIGHUP reload that hits malformed material
    // must surface as Err *and* leave the live listener serving the
    // previous cert. The runbook tells operators to rely on this when a
    // half-written cert lands on disk; without a test it's just a hope.
    let bundle_a = mint_bundle();
    let mode_a = TlsMode::Mtls {
        cert: bundle_a.server_cert_path.clone(),
        key: bundle_a.server_key_path.clone(),
        client_ca: bundle_a.ca_path.clone(),
    };
    let server = start_rest(&mode_a).await;
    let url = format!("https://localhost:{}/v1/pairs", server.addr.port());

    let client_a = reqwest_client_mtls(
        &bundle_a.ca_pem,
        &bundle_a.client.pem,
        &bundle_a.client.key_pem,
    );
    wait_rest_ready(&client_a, &url).await;
    assert_eq!(client_a.get(&url).send().await.unwrap().status(), 200);

    // Stage garbage PEM files and attempt the reload through the same
    // path SIGHUP triggers in production.
    let garbage_dir = tempfile::tempdir().unwrap();
    let garbage_cert = garbage_dir.path().join("cert.pem");
    let garbage_key = garbage_dir.path().join("key.pem");
    let garbage_ca = garbage_dir.path().join("ca.pem");
    std::fs::write(
        &garbage_cert,
        b"-----BEGIN CERTIFICATE-----\nnot a cert\n-----END CERTIFICATE-----\n",
    )
    .unwrap();
    std::fs::write(
        &garbage_key,
        b"-----BEGIN PRIVATE KEY-----\nnot a key\n-----END PRIVATE KEY-----\n",
    )
    .unwrap();
    std::fs::write(
        &garbage_ca,
        b"-----BEGIN CERTIFICATE-----\nnot a ca\n-----END CERTIFICATE-----\n",
    )
    .unwrap();
    let bad_mode = TlsMode::Mtls {
        cert: garbage_cert,
        key: garbage_key,
        client_ca: garbage_ca,
    };

    let tls_cfg_handle = server.tls_cfg.clone().expect("tls config present");
    let outcome = tls::reload_axum_rustls(&tls_cfg_handle, &bad_mode).await;
    assert!(
        outcome.is_err(),
        "garbage PEM must surface as reload Err, got {outcome:?}"
    );

    // Old material must still serve. Use a fresh reqwest client so the
    // assertion isn't masked by a kept-alive connection from before.
    let client_a_after = reqwest_client_mtls(
        &bundle_a.ca_pem,
        &bundle_a.client.pem,
        &bundle_a.client.key_pem,
    );
    let resp = client_a_after
        .get(&url)
        .send()
        .await
        .expect("post-failed-reload GET");
    assert_eq!(resp.status(), 200);

    server.shutdown().await;
}
