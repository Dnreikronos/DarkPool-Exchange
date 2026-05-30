//! End-to-end boot tests that exercise the real darkpool-server transports:
//! a tonic gRPC server on a real TCP listener with a real tonic client, and
//! an axum REST server reached through a raw HTTP/1.1 request over TCP.
//! All other tests in this crate drive the handler in-process.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use dp_api::auth::{AuthLayer, AUTH_HEADER};
use dp_api::handler::ApiHandler;
use dp_api::pb::dark_pool_service_client::DarkPoolServiceClient;
use dp_api::pb::dark_pool_service_server::DarkPoolServiceServer;
use dp_api::pb::{ListPairsRequest, PlaceOrderRequest};
use dp_api::ratelimit::RateLimitLayer;
use dp_api::rest;
use dp_engine::Engine;
use dp_event::MemStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};

fn new_handler() -> ApiHandler {
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

#[tokio::test]
async fn grpc_server_accepts_real_client() {
    let handler = new_handler();
    let (listener, addr) = bind_local().await;
    let cancel = CancellationToken::new();

    let server_cancel = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server_task = tokio::spawn(async move {
        Server::builder()
            .add_service(DarkPoolServiceServer::new(handler))
            .serve_with_incoming_shutdown(incoming, async move { server_cancel.cancelled().await })
            .await
            .expect("grpc server");
    });

    // Connect via real channel.
    let endpoint = format!("http://{}", addr);
    let channel = {
        let mut last_err: Option<tonic::transport::Error> = None;
        let mut ch = None;
        for _ in 0..50 {
            match Channel::from_shared(endpoint.clone())
                .unwrap()
                .connect()
                .await
            {
                Ok(c) => {
                    ch = Some(c);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
        ch.unwrap_or_else(|| panic!("connect: {:?}", last_err))
    };

    let mut client = DarkPoolServiceClient::new(channel);

    // Empty payload → InvalidArgument from validation. Verifies the request
    // crossed the wire and reached the handler's validation layer.
    let err = client
        .place_order(PlaceOrderRequest {
            commitment: vec![],
            proof: vec![],
            encrypted_payload: vec![],
        })
        .await
        .expect_err("empty PlaceOrder should fail validation");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // Valid public read crossing the wire: the registered pair is listed.
    let resp = client
        .list_pairs(ListPairsRequest {})
        .await
        .expect("list_pairs")
        .into_inner();
    assert!(resp.pairs.iter().any(|p| p.pair == "ETH/USDC"));

    cancel.cancel();
    server_task.await.expect("join grpc task");
}

#[tokio::test]
async fn grpc_server_with_layers_enforces_auth_and_ratelimit() {
    // Mirrors main.rs wiring: tonic Server with auth + rate-limit tower layers.
    // Ensures middleware regressions surface in integration, not just unit tests.
    let handler = new_handler();
    let (listener, addr) = bind_local().await;
    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();

    let auth = AuthLayer::new(vec!["secret".into()]);
    // burst=1, rate=0 → first req passes, second is rejected.
    let rl = RateLimitLayer::new(0.0, 1.0, Duration::from_secs(60));

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server_task = tokio::spawn(async move {
        Server::builder()
            .layer(auth)
            .layer(rl)
            .add_service(DarkPoolServiceServer::new(handler))
            .serve_with_incoming_shutdown(incoming, async move { server_cancel.cancelled().await })
            .await
            .expect("grpc server");
    });

    let endpoint = format!("http://{}", addr);
    let channel = {
        let mut ch = None;
        for _ in 0..50 {
            if let Ok(c) = Channel::from_shared(endpoint.clone())
                .unwrap()
                .connect()
                .await
            {
                ch = Some(c);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        ch.expect("connect")
    };

    // No api key → Unauthenticated.
    let mut anon = DarkPoolServiceClient::new(channel.clone());
    let err = anon
        .list_pairs(ListPairsRequest {})
        .await
        .expect_err("missing key must be rejected");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);

    // With valid key → first request OK, second exhausts rate-limit bucket.
    let key: MetadataValue<_> = "secret".parse().unwrap();
    let mut authed = DarkPoolServiceClient::with_interceptor(
        channel,
        #[allow(clippy::result_large_err)]
        move |mut req: tonic::Request<()>| {
            req.metadata_mut().insert(AUTH_HEADER, key.clone());
            Ok(req)
        },
    );
    authed
        .list_pairs(ListPairsRequest {})
        .await
        .expect("first authed request");
    let err = authed
        .list_pairs(ListPairsRequest {})
        .await
        .expect_err("second request must be rate-limited");
    assert_eq!(err.code(), tonic::Code::ResourceExhausted);

    cancel.cancel();
    server_task.await.expect("join grpc task");
}

#[tokio::test]
async fn rest_server_accepts_real_http() {
    let handler = Arc::new(new_handler());
    let app = rest::router(handler);

    let (listener, addr) = bind_local().await;
    let cancel = CancellationToken::new();
    let shutdown_cancel = cancel.clone();

    let server_task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move { shutdown_cancel.cancelled().await })
        .await
        .expect("rest server");
    });

    let body = http_get(addr, "/v1/pairs").await;
    let (status, payload) = parse_http_response(&body);
    assert_eq!(status, 200, "raw response: {body}");
    let json: serde_json::Value = serde_json::from_str(payload).expect("rest body should be JSON");
    let pairs = json["pairs"].as_array().expect("pairs array");
    assert!(pairs.iter().any(|p| p["pair"] == "ETH/USDC"));

    cancel.cancel();
    server_task.await.expect("join rest task");
}

async fn http_get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect rest");
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    stream.write_all(req.as_bytes()).await.expect("write req");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read resp");
    String::from_utf8(buf).expect("utf-8 response")
}

fn parse_http_response(raw: &str) -> (u16, &str) {
    let (head, body) = raw.split_once("\r\n\r\n").expect("http header/body split");
    let status_line = head.lines().next().expect("status line");
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status code");
    // Strip Transfer-Encoding: chunked framing if present.
    let payload = if head
        .lines()
        .any(|l| l.eq_ignore_ascii_case("Transfer-Encoding: chunked"))
    {
        strip_chunks(body)
    } else {
        body
    };
    (code, payload)
}

fn strip_chunks(body: &str) -> &str {
    // axum 0.7 uses Content-Length for fixed-size JSON, so this rarely fires.
    // Keep it minimal: take everything between the first chunk-size line and
    // the terminating "0\r\n".
    if let Some((_size, rest)) = body.split_once("\r\n") {
        if let Some((payload, _)) = rest.split_once("\r\n0\r\n") {
            return payload;
        }
        return rest;
    }
    body
}
