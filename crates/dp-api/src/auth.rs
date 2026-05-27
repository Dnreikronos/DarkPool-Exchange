use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use alloy_primitives::Address;
use axum::extract::{Request as AxumRequest, State as AxumState};
use axum::middleware::Next;
use axum::response::Response as AxumResponse;
use http::{HeaderMap, Request, Response};
use tonic::body::BoxBody;
use tonic::Status;
use tower::{Layer, Service};

use crate::validation::{MSG_INVALID_API_KEY, MSG_MISSING_API_KEY};

pub const AUTH_HEADER: &str = "x-api-key";

#[derive(Clone, Debug)]
pub enum AuthenticatedIdentity {
    ApiKey,
    Wallet(Address),
}

#[derive(Clone, Debug)]
pub struct AuthCore {
    keys: Arc<HashSet<String>>,
    jwt: Option<Arc<crate::siwe::JwtManager>>,
}

impl AuthCore {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            keys: Arc::new(keys.into_iter().filter(|k| !k.is_empty()).collect()),
            jwt: None,
        }
    }

    pub fn new_with_jwt(keys: Vec<String>, jwt: Arc<crate::siwe::JwtManager>) -> Self {
        Self {
            keys: Arc::new(keys.into_iter().filter(|k| !k.is_empty()).collect()),
            jwt: Some(jwt),
        }
    }

    pub fn check(&self, headers: &HeaderMap) -> Result<AuthenticatedIdentity, Status> {
        if let Some(auth_val) = headers.get(http::header::AUTHORIZATION) {
            if let Some(jwt) = &self.jwt {
                let auth_str = auth_val
                    .to_str()
                    .map_err(|_| Status::unauthenticated("invalid authorization header"))?;
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    let claims = jwt.verify(token).map_err(|e| {
                        Status::unauthenticated(format!("invalid token: {e}"))
                    })?;
                    let addr = crate::siwe::JwtManager::address_from_claims(&claims)
                        .ok_or_else(|| {
                            Status::unauthenticated("invalid address in token")
                        })?;
                    return Ok(AuthenticatedIdentity::Wallet(addr));
                }
            }
        }

        if self.keys.is_empty() {
            return Ok(AuthenticatedIdentity::ApiKey);
        }
        let key = match headers.get(AUTH_HEADER) {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(_) => return Err(Status::unauthenticated(MSG_INVALID_API_KEY)),
            },
            None => return Err(Status::unauthenticated(MSG_MISSING_API_KEY)),
        };
        if key.is_empty() {
            return Err(Status::unauthenticated(MSG_MISSING_API_KEY));
        }
        if !self.keys.contains(key) {
            return Err(Status::unauthenticated(MSG_INVALID_API_KEY));
        }
        Ok(AuthenticatedIdentity::ApiKey)
    }
}

#[derive(Clone, Debug)]
pub struct AuthLayer {
    core: AuthCore,
}

impl AuthLayer {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            core: AuthCore::new(keys),
        }
    }

    pub fn from_core(core: AuthCore) -> Self {
        Self { core }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            core: self.core.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthService<S> {
    inner: S,
    core: AuthCore,
}

impl<S, B> Service<Request<B>> for AuthService<S>
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

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let core = self.core.clone();
        Box::pin(async move {
            match core.check(req.headers()) {
                Ok(identity) => {
                    req.extensions_mut().insert(identity);
                    inner.call(req).await
                }
                Err(status) => Ok(status.into_http()),
            }
        })
    }
}

pub async fn auth_axum_mw(
    AxumState(core): AxumState<AuthCore>,
    mut req: AxumRequest,
    next: Next,
) -> AxumResponse {
    if req.headers().get(AUTH_HEADER).is_none() {
        if let Some(key) = extract_api_key_from_query(req.uri().query()) {
            if let Ok(val) = http::HeaderValue::from_str(&key) {
                req.headers_mut()
                    .insert(http::HeaderName::from_static(AUTH_HEADER), val);
            }
        }
    }
    match core.check(req.headers()) {
        Ok(identity) => {
            req.extensions_mut().insert(identity);
            next.run(req).await
        }
        Err(status) => crate::rest::status_to_response(status),
    }
}

fn extract_api_key_from_query(query: Option<&str>) -> Option<String> {
    let q = query?;
    for param in q.split('&') {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        if key == "apiKey" && !value.is_empty() {
            return Some(
                percent_encoding::percent_decode_str(value)
                    .decode_utf8_lossy()
                    .into_owned(),
            );
        }
    }
    None
}
