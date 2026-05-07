use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request as AxumRequest, State as AxumState};
use axum::middleware::Next;
use axum::response::Response as AxumResponse;
use http::{HeaderMap, Request, Response};
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;
use tonic::body::BoxBody;
use tonic::transport::server::TcpConnectInfo;
use tonic::Status;
use tower::{Layer, Service};

use crate::auth::AUTH_HEADER;
use crate::validation::MSG_RATE_LIMIT_EXCEEDED;

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_fill: Instant,
}

#[derive(Debug)]
struct State {
    buckets: HashMap<String, Bucket>,
    rate: f64,
    capacity: f64,
    stale_after: Duration,
}

#[derive(Clone, Debug)]
pub struct RateLimitCore {
    state: Arc<Mutex<State>>,
}

impl RateLimitCore {
    pub fn new(rate: f64, burst: f64, stale_after: Duration) -> Self {
        let stale_after = if stale_after.is_zero() {
            Duration::from_secs(600)
        } else {
            stale_after
        };
        Self {
            state: Arc::new(Mutex::new(State {
                buckets: HashMap::new(),
                rate,
                capacity: burst,
                stale_after,
            })),
        }
    }

    pub fn allow(&self, key: &str) -> Result<(), Status> {
        let mut s = self.state.lock();
        let now = Instant::now();
        let rate = s.rate;
        let capacity = s.capacity;
        let bucket = s.buckets.entry(key.to_string()).or_insert_with(|| Bucket {
            tokens: capacity,
            last_fill: now,
        });
        let elapsed = now
            .saturating_duration_since(bucket.last_fill)
            .as_secs_f64();
        bucket.tokens += elapsed * rate;
        if bucket.tokens > capacity {
            bucket.tokens = capacity;
        }
        bucket.last_fill = now;
        if bucket.tokens < 1.0 {
            return Err(Status::resource_exhausted(MSG_RATE_LIMIT_EXCEEDED));
        }
        bucket.tokens -= 1.0;
        Ok(())
    }

    pub fn evict_stale(&self) {
        let mut s = self.state.lock();
        let now = Instant::now();
        let stale = s.stale_after;
        s.buckets
            .retain(|_, b| now.saturating_duration_since(b.last_fill) <= stale);
    }

    pub fn bucket_count(&self) -> usize {
        self.state.lock().buckets.len()
    }

    pub fn start_cleanup(&self, cancel: CancellationToken, interval: Duration) {
        let core = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip immediate
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = ticker.tick() => core.evict_stale(),
                }
            }
        });
    }
}

pub fn client_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if let Some(v) = headers.get(AUTH_HEADER) {
        if let Ok(s) = v.to_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    if let Some(addr) = peer {
        return addr.ip().to_string();
    }
    "anonymous".to_string()
}

#[derive(Clone, Debug)]
pub struct RateLimitLayer {
    core: RateLimitCore,
}

impl RateLimitLayer {
    pub fn new(rate: f64, burst: f64, stale_after: Duration) -> Self {
        Self {
            core: RateLimitCore::new(rate, burst, stale_after),
        }
    }

    pub fn core(&self) -> RateLimitCore {
        self.core.clone()
    }

    pub fn from_core(core: RateLimitCore) -> Self {
        Self { core }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            core: self.core.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RateLimitService<S> {
    inner: S,
    core: RateLimitCore,
}

impl<S, B> Service<Request<B>> for RateLimitService<S>
where
    S: Service<Request<B>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let core = self.core.clone();
        Box::pin(async move {
            let peer = req
                .extensions()
                .get::<TcpConnectInfo>()
                .and_then(|i| i.remote_addr())
                .or_else(|| req.extensions().get::<SocketAddr>().copied())
                .or_else(|| {
                    req.extensions()
                        .get::<ConnectInfo<SocketAddr>>()
                        .map(|ci| ci.0)
                });
            let key = client_key(req.headers(), peer);
            if let Err(status) = core.allow(&key) {
                return Ok(status.into_http());
            }
            inner.call(req).await
        })
    }
}

pub async fn ratelimit_axum_mw(
    AxumState(core): AxumState<RateLimitCore>,
    req: AxumRequest,
    next: Next,
) -> AxumResponse {
    let peer = req.extensions().get::<SocketAddr>().copied().or_else(|| {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0)
    });
    let key = client_key(req.headers(), peer);
    if let Err(status) = core.allow(&key) {
        return crate::rest::status_to_response(status);
    }
    next.run(req).await
}
