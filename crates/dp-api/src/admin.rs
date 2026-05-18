use std::str::FromStr;

use alloy_primitives::Address;
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

        let cfg = PairConfig {
            base_token: base,
            quote_token: quote,
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
