use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request as AxumRequest, State as AxumState};
use axum::middleware::Next;
use axum::response::Response as AxumResponse;
use http::{HeaderMap, Request, Response};
use ipnet::IpNet;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;
use tonic::body::BoxBody;
use tonic::transport::server::TcpConnectInfo;
use tonic::Status;
use tower::{Layer, Service};

use crate::auth::{AuthenticatedIdentity, AUTH_HEADER};
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
    trusted: TrustedProxies,
}

impl RateLimitCore {
    pub fn new(rate: f64, burst: f64, stale_after: Duration) -> Self {
        Self::with_trusted_proxies(rate, burst, stale_after, TrustedProxies::none())
    }

    /// As [`RateLimitCore::new`], but treats `trusted` as reverse-proxy
    /// source ranges whose forwarding headers are believed when keying
    /// requests (see [`TrustedProxies`]).
    pub fn with_trusted_proxies(
        rate: f64,
        burst: f64,
        stale_after: Duration,
        trusted: TrustedProxies,
    ) -> Self {
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
            trusted,
        }
    }

    pub fn trusted_proxies(&self) -> &TrustedProxies {
        &self.trusted
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

/// Set of trusted reverse-proxy / load-balancer source ranges. When the
/// TCP peer falls inside one of these, the limiter believes the
/// `X-Forwarded-For` / `X-Real-IP` headers that proxy sets and keys on the
/// real client IP instead of the proxy IP. Empty (the default) means
/// **directly exposed**: forwarding headers are ignored entirely and the
/// peer IP is authoritative — `X-Forwarded-For` is attacker-controlled
/// when no trusted proxy sits in front, so trusting it unconditionally
/// would let any caller forge their rate-limit key.
#[derive(Clone, Debug, Default)]
pub struct TrustedProxies {
    nets: Arc<Vec<IpNet>>,
}

impl TrustedProxies {
    /// The directly-exposed default: no proxy is trusted.
    pub fn none() -> Self {
        Self::default()
    }

    /// Parse a comma/whitespace-separated list of CIDRs or bare IPs
    /// (e.g. `"10.0.0.0/8, 192.168.1.1, ::1"`). A bare IP becomes a host
    /// route (`/32` for IPv4, `/128` for IPv6). Blank entries are skipped;
    /// an unparseable entry is a hard error so a typo fails boot loudly
    /// rather than silently trusting nothing.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut nets = Vec::new();
        for tok in spec.split([',', ' ', '\t', '\n']) {
            let tok = tok.trim();
            if tok.is_empty() {
                continue;
            }
            let net = if tok.contains('/') {
                tok.parse::<IpNet>()
                    .map_err(|e| format!("invalid trusted-proxy CIDR '{tok}': {e}"))?
            } else {
                let ip = tok
                    .parse::<IpAddr>()
                    .map_err(|e| format!("invalid trusted-proxy IP '{tok}': {e}"))?;
                let prefix = if ip.is_ipv4() { 32 } else { 128 };
                IpNet::new(ip, prefix).expect("host prefix length is always valid")
            };
            nets.push(net);
        }
        Ok(Self {
            nets: Arc::new(nets),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.nets.is_empty()
    }

    fn contains(&self, ip: IpAddr) -> bool {
        self.nets.iter().any(|n| n.contains(&ip))
    }
}

/// Resolve the client IP the limiter should key on.
///
/// With no trusted proxies, this is always the TCP peer. When the peer is
/// a trusted proxy, walk its `X-Forwarded-For` chain right-to-left and
/// return the rightmost address that is **not** itself a trusted hop —
/// that is the real client as seen by the outermost proxy we trust, and
/// the only entry an attacker upstream of our proxies cannot forge. Falls
/// back to `X-Real-IP`, then the peer IP, when no usable forwarding header
/// is present. Returns `None` only when there is no peer at all (e.g. unit
/// tests using `oneshot` without `ConnectInfo`).
pub fn resolve_client_ip(
    trusted: &TrustedProxies,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
) -> Option<IpAddr> {
    let peer_ip = peer?.ip();
    if trusted.is_empty() || !trusted.contains(peer_ip) {
        return Some(peer_ip);
    }
    if let Some(ip) = forwarded_for_client(trusted, headers) {
        return Some(ip);
    }
    if let Some(ip) = real_ip(headers) {
        return Some(ip);
    }
    Some(peer_ip)
}

/// Rightmost untrusted entry of the `X-Forwarded-For` chain. If every hop
/// is trusted, the originating client is the leftmost entry.
fn forwarded_for_client(trusted: &TrustedProxies, headers: &HeaderMap) -> Option<IpAddr> {
    let mut chain: Vec<IpAddr> = Vec::new();
    for v in headers.get_all("x-forwarded-for") {
        let Ok(s) = v.to_str() else { continue };
        for part in s.split(',') {
            if let Some(ip) = parse_forwarded_ip(part) {
                chain.push(ip);
            }
        }
    }
    if chain.is_empty() {
        return None;
    }
    chain
        .iter()
        .rev()
        .find(|ip| !trusted.contains(**ip))
        .or_else(|| chain.first())
        .copied()
}

fn real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    parse_forwarded_ip(headers.get("x-real-ip")?.to_str().ok()?)
}

/// Parse a single forwarded-for token, tolerating an optional `:port`
/// suffix (`1.2.3.4:55`, `[::1]:55`) that some proxies append.
fn parse_forwarded_ip(s: &str) -> Option<IpAddr> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Ok(sa) = s.parse::<SocketAddr>() {
        return Some(sa.ip());
    }
    // Bare IPv4 with a port that didn't parse as SocketAddr: strip it.
    if let Some((host, _port)) = s.rsplit_once(':') {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Some(ip);
        }
    }
    None
}

pub fn client_key(
    identity: Option<&AuthenticatedIdentity>,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trusted: &TrustedProxies,
) -> String {
    if let Some(id) = identity {
        match id {
            AuthenticatedIdentity::Wallet(addr) => return format!("{:#x}", addr),
            AuthenticatedIdentity::ApiKey => {}
        }
    }
    if let Some(v) = headers.get(AUTH_HEADER) {
        if let Ok(s) = v.to_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    if let Some(ip) = resolve_client_ip(trusted, headers, peer) {
        return ip.to_string();
    }
    "anonymous".to_string()
}

/// Bucket key for unauthenticated routes (SIWE auth, ops). Keys by client
/// IP — never the `x-api-key` header, which is attacker-controlled on
/// routes with no auth layer and would otherwise let a caller mint
/// unlimited buckets by rotating the header value. The client IP is the
/// TCP peer unless that peer is a trusted proxy, in which case it is
/// resolved from the forwarding headers (see [`resolve_client_ip`]). Falls
/// back to a single shared `"anonymous"` bucket only when the peer address
/// is unavailable (e.g. unit tests using `oneshot` without `ConnectInfo`).
pub fn ip_client_key(
    trusted: &TrustedProxies,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
) -> String {
    match resolve_client_ip(trusted, headers, peer) {
        Some(ip) => ip.to_string(),
        None => "anonymous".to_string(),
    }
}

/// The resolved IP rate-limit key, recorded in request extensions by
/// [`ratelimit_ip_axum_mw`] so downstream handlers (the SIWE nonce
/// endpoint) can reuse the same key for their own per-IP accounting
/// without re-deriving it.
#[derive(Clone, Debug)]
pub struct ClientKey(pub String);

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
            let identity = req.extensions().get::<AuthenticatedIdentity>().cloned();
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
            let key = client_key(
                identity.as_ref(),
                req.headers(),
                peer,
                core.trusted_proxies(),
            );
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
    let identity = req.extensions().get::<AuthenticatedIdentity>().cloned();
    let peer = req.extensions().get::<SocketAddr>().copied().or_else(|| {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0)
    });
    let key = client_key(
        identity.as_ref(),
        req.headers(),
        peer,
        core.trusted_proxies(),
    );
    if let Err(status) = core.allow(&key) {
        return crate::rest::status_to_response(status);
    }
    next.run(req).await
}

/// Rate-limit middleware for unauthenticated routes (SIWE auth, ops),
/// keyed strictly by peer IP via [`ip_client_key`]. Unlike
/// [`ratelimit_axum_mw`] it ignores both the authenticated identity
/// (there is none on these routes) and the `x-api-key` header (untrusted
/// here). On success it records the resolved key as a [`ClientKey`]
/// request extension for downstream per-IP accounting.
pub async fn ratelimit_ip_axum_mw(
    AxumState(core): AxumState<RateLimitCore>,
    mut req: AxumRequest,
    next: Next,
) -> AxumResponse {
    let peer = req.extensions().get::<SocketAddr>().copied().or_else(|| {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0)
    });
    let key = ip_client_key(core.trusted_proxies(), req.headers(), peer);
    if let Err(status) = core.allow(&key) {
        return crate::rest::status_to_response(status);
    }
    req.extensions_mut().insert(ClientKey(key));
    next.run(req).await
}
