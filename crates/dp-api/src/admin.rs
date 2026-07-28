use std::str::FromStr;

use alloy_primitives::Address;
use dp_crypto::{decrypter_from_uri, validate_key_id, KeyEntry, KeyStatus, MultiKeyDecrypter};
use dp_engine::{Engine, PairConfig, PairStatus};
use rust_decimal::Decimal;
use tonic::{Request, Response, Status};

use crate::conv::pair_config_to_proto;
use crate::error::engine_error_to_status;
use crate::pb::dark_pool_admin_service_server::DarkPoolAdminService;
use crate::pb::{
    DelistPairRequest, DelistPairResponse, ListPairsAdminRequest, ListPairsAdminResponse, PairInfo,
    RegisterPairRequest, RegisterPairResponse, SuspendPairRequest, SuspendPairResponse,
};

#[derive(Clone)]
pub struct AdminApiHandler {
    pub engine: Engine,
}

impl AdminApiHandler {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }
}

/// Key-rotation admin surface. Held separately from `AdminApiHandler`
/// because it doesn't ride on the tonic gRPC service set and only
/// needs the `MultiKeyDecrypter` handle. Kept `Clone` so axum's
/// `State<Arc<KeyAdminHandler>>` extraction works the same as the
/// existing admin routes.
#[derive(Clone)]
pub struct KeyAdminHandler {
    pub multi: MultiKeyDecrypter,
}

impl KeyAdminHandler {
    pub fn new(multi: MultiKeyDecrypter) -> Self {
        Self { multi }
    }

    /// Resolve a URI → ECIES decrypter and register it with `status`.
    /// Replaces any existing entry of the same `id` so the same
    /// endpoint can promote (Rotating → Active) or demote
    /// (Active → Rotating) without a separate route.
    pub fn upsert(&self, id: String, uri: &str, status: KeyStatus) -> Result<(), AdminKeyError> {
        // Gate at admission so a bad id never reaches the metric label
        // set. Auto-derived ids (`key-XXXXXX`, `primary`) pass by
        // construction; operator-supplied ids must conform.
        validate_key_id(&id).map_err(|e| AdminKeyError::Invalid(e.to_string()))?;
        let decrypter =
            decrypter_from_uri(uri).map_err(|e| AdminKeyError::Resolve(e.to_string()))?;
        self.multi.insert(KeyEntry::new(id, status, decrypter));
        Ok(())
    }

    pub fn remove(&self, id: &str) -> bool {
        self.multi.remove(id)
    }

    pub fn sunset(&self, id: &str) -> bool {
        self.multi.set_status(id, KeyStatus::Sunset)
    }

    pub fn list(&self) -> Vec<(String, KeyStatus)> {
        self.multi.list()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdminKeyError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("key not found: {0}")]
    NotFound(String),
    #[error("key resolution failed: {0}")]
    Resolve(String),
}

fn parse_address(s: &str, field: &str) -> Result<Address, Status> {
    Address::from_str(s.trim())
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn parse_decimal_nonneg(s: &str, field: &str) -> Result<Decimal, Status> {
    let d = Decimal::from_str(s.trim())
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))?;
    if d.is_sign_negative() {
        return Err(Status::invalid_argument(format!("{field} must be >= 0")));
    }
    Ok(d)
}

/// Validate an optional proto `decimals` field into a `u8`. Absent defaults to
/// 18 (the ERC20 default); values above 30 are rejected — no real ERC20 exceeds
/// that, and the settlement-side `10**decimals` would be nonsensical (#211).
fn parse_decimals(v: Option<u32>, field: &str) -> Result<u8, Status> {
    match v {
        None => Ok(18),
        Some(d) if d <= 30 => Ok(d as u8),
        Some(d) => Err(Status::invalid_argument(format!(
            "{field} must be <= 30, got {d}"
        ))),
    }
}

#[tonic::async_trait]
impl DarkPoolAdminService for AdminApiHandler {
    async fn register_pair(
        &self,
        req: Request<RegisterPairRequest>,
    ) -> Result<Response<RegisterPairResponse>, Status> {
        let req = req.into_inner();
        // Empty / whitespace-only `pair` is caught by `Pair::parse` inside
        // `Engine::register_pair_with_event` (returns `PairRequired`); no
        // separate guard needed here.
        let base = parse_address(&req.base_token, "base_token")?;
        let quote = parse_address(&req.quote_token, "quote_token")?;
        let min_order_size = parse_decimal_nonneg(&req.min_order_size, "min_order_size")?;
        // tick_size = 0 is the documented "no tick check" sentinel (see
        // PairConfig in dp_engine::state), so non-negative is the right gate.
        let tick_size = parse_decimal_nonneg(&req.tick_size, "tick_size")?;
        if matches!(req.auction_interval_ms, Some(0)) {
            return Err(Status::invalid_argument("auction_interval_ms must be > 0"));
        }
        let auction_interval = req
            .auction_interval_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64));
        let base_decimals = parse_decimals(req.base_decimals, "base_decimals")?;
        let quote_decimals = parse_decimals(req.quote_decimals, "quote_decimals")?;

        let cfg = PairConfig {
            base_token: base,
            quote_token: quote,
            base_decimals,
            quote_decimals,
            min_order_size,
            tick_size,
            auction_interval,
            status: PairStatus::Active,
        };
        let canonical = self
            .engine
            .register_pair_with_event(&req.pair, cfg.clone())
            .map_err(engine_error_to_status)?;

        let info = pair_config_to_proto(canonical.as_str(), &cfg);
        Ok(Response::new(RegisterPairResponse { pair: Some(info) }))
    }

    async fn suspend_pair(
        &self,
        req: Request<SuspendPairRequest>,
    ) -> Result<Response<SuspendPairResponse>, Status> {
        let req = req.into_inner();
        if req.pair.is_empty() {
            return Err(Status::invalid_argument("pair is required"));
        }
        self.engine
            .suspend_pair(&req.pair)
            .map_err(engine_error_to_status)?;
        Ok(Response::new(SuspendPairResponse {}))
    }

    async fn delist_pair(
        &self,
        req: Request<DelistPairRequest>,
    ) -> Result<Response<DelistPairResponse>, Status> {
        let req = req.into_inner();
        if req.pair.is_empty() {
            return Err(Status::invalid_argument("pair is required"));
        }
        let cancelled = self
            .engine
            .delist_pair(&req.pair)
            .map_err(engine_error_to_status)?;
        Ok(Response::new(DelistPairResponse {
            cancelled_orders: cancelled as u32,
        }))
    }

    async fn list_pairs_admin(
        &self,
        _req: Request<ListPairsAdminRequest>,
    ) -> Result<Response<ListPairsAdminResponse>, Status> {
        let mut pairs: Vec<PairInfo> = self
            .engine
            .list_pairs()
            .into_iter()
            .map(|(p, c)| pair_config_to_proto(&p, &c))
            .collect();
        pairs.sort_by(|a, b| a.pair.cmp(&b.pair));
        Ok(Response::new(ListPairsAdminResponse { pairs }))
    }
}

#[cfg(test)]
mod key_admin_tests {
    //! Round-trip checks for the key-rotation surface. The REST layer
    //! is the operator's only entry point, but the underlying handler
    //! is what guarantees the (resolve URI → register → drain →
    //! delete) lifecycle stays coherent.

    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fresh_handler() -> KeyAdminHandler {
        KeyAdminHandler::new(MultiKeyDecrypter::new())
    }

    fn write_hex_key_file(content: &[u8; 32]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(hex::encode(content).as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn upsert_register_and_list() {
        let h = fresh_handler();
        let f = write_hex_key_file(&[0x11; 32]);
        let uri = format!("file:{}", f.path().display());
        h.upsert("active-2026q2".into(), &uri, KeyStatus::Active)
            .unwrap();
        let listed = h.list();
        assert_eq!(listed, vec![("active-2026q2".into(), KeyStatus::Active)]);
    }

    #[test]
    fn upsert_replaces_existing() {
        let h = fresh_handler();
        let f = write_hex_key_file(&[0x22; 32]);
        let uri = format!("file:{}", f.path().display());
        h.upsert("k".into(), &uri, KeyStatus::Active).unwrap();
        h.upsert("k".into(), &uri, KeyStatus::Rotating).unwrap();
        assert_eq!(h.list().len(), 1);
        assert_eq!(h.list()[0].1, KeyStatus::Rotating);
    }

    #[test]
    fn sunset_transitions_status() {
        let h = fresh_handler();
        let f = write_hex_key_file(&[0x33; 32]);
        let uri = format!("file:{}", f.path().display());
        h.upsert("old".into(), &uri, KeyStatus::Active).unwrap();
        assert!(h.sunset("old"));
        assert_eq!(h.list()[0].1, KeyStatus::Sunset);
        assert!(!h.sunset("missing"));
    }

    #[test]
    fn remove_drops_entry() {
        let h = fresh_handler();
        let f = write_hex_key_file(&[0x44; 32]);
        let uri = format!("file:{}", f.path().display());
        h.upsert("doomed".into(), &uri, KeyStatus::Sunset).unwrap();
        assert!(h.remove("doomed"));
        assert!(h.list().is_empty());
        assert!(!h.remove("doomed"));
    }

    #[test]
    fn upsert_unknown_scheme_returns_resolve_error() {
        let h = fresh_handler();
        match h.upsert("k".into(), "vault://x", KeyStatus::Active) {
            Err(AdminKeyError::Resolve(msg)) => assert!(msg.contains("unsupported")),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
