use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use rust_decimal::Decimal;

use crate::abi::DarkPool;
use crate::SettlementError;

/// Boxed future returned by [`BalanceOracle::lookup`]. Aliased to keep the
/// trait and its impls readable (and to satisfy `clippy::type_complexity`).
pub type BalanceLookupFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(Decimal, i128), SettlementError>> + Send + 'a>>;

/// Per-leg solvency/position source for the matching circuit's family-7
/// (solvency) and family-8 (position) constraints.
///
/// **Per-asset semantics (#170).** The two legs of a match settle different
/// assets, so the balance is denominated in the asset the leg *spends*: a bid
/// (buyer) pays the quote token, an ask (seller) delivers the base token. The
/// caller resolves the spend asset from the pair registry and passes its
/// address and on-chain `decimals`, so the returned `Decimal` is in whole-token
/// units the circuit can scale into its 1e8 fixed-point domain.
///
/// `lookup` is fallible and async: a real oracle reads chain state and must be
/// able to fail closed (reject the order) rather than fabricate a balance when
/// the read fails.
pub trait BalanceOracle: Send + Sync {
    fn lookup<'a>(
        &'a self,
        trader: Address,
        asset: Address,
        decimals: u8,
    ) -> BalanceLookupFuture<'a>;
}

/// Returns a hard-coded 1B balance / 0 position for any trader, ignoring the
/// asset. Solvency constraints (family 7) pass trivially under this oracle, so
/// it proves nothing about real funds. NOT FOR PRODUCTION — it exists only so
/// dev/test deployments without a chain connection can exercise the matching
/// path. A value-bearing deployment must install [`ChainBalanceOracle`] and
/// refuse to boot on this one (the `dp-api` boot path enforces that).
pub struct InsecureDevOracle;

impl BalanceOracle for InsecureDevOracle {
    fn lookup<'a>(
        &'a self,
        _trader: Address,
        _asset: Address,
        _decimals: u8,
    ) -> BalanceLookupFuture<'a> {
        Box::pin(async { Ok((Decimal::from(1_000_000_000u64), 0i128)) })
    }
}

/// Reads a trader's settlement-locked collateral from the on-chain
/// `reserved[trader][asset]` mapping of the `DarkPool` contract. Only reserved
/// funds are eligible for matching and settlement (the contract debits
/// `reserved` in `_consumeReserved` and reverts the whole batch on underflow),
/// so binding the circuit's solvency witness to the same mapping is what stops
/// an honest batch from being matched against funds that aren't there.
///
/// **Position (family 8) is not yet sourced.** The contract tracks no net
/// position, so `lookup` returns `0` for position and the in-circuit position
/// limit remains effectively unenforced. Wiring a real position source needs
/// on-chain position accounting and is tracked as a separate follow-up; this
/// oracle deliberately does not pretend otherwise.
///
/// **Reports total reserved, not per-order available.** `reserved` is the
/// trader's whole settlement-locked bucket for the asset, so each of N
/// concurrent orders witnesses the full balance and the solvency check
/// over-approximates what is actually free. The on-chain `_consumeReserved`
/// debit is the real guard against over-spend; trustless per-order accounting
/// would need a committed reserved-state root (out of scope, see #213).
pub struct ChainBalanceOracle<P> {
    provider: P,
    contract: Address,
}

impl<P: Provider + Send + Sync> ChainBalanceOracle<P> {
    pub fn new(provider: P, contract: Address) -> Self {
        Self { provider, contract }
    }
}

/// Wall-clock bound on a single `reserved` read. Order placement awaits this
/// synchronously and the HTTP transport has no default timeout, so without a
/// cap a hung RPC would stall placement indefinitely — the opposite of the
/// fail-closed contract. On elapse the lookup errors and the order is rejected.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

impl<P: Provider + Send + Sync + 'static> BalanceOracle for ChainBalanceOracle<P> {
    fn lookup<'a>(
        &'a self,
        trader: Address,
        asset: Address,
        decimals: u8,
    ) -> BalanceLookupFuture<'a> {
        Box::pin(async move {
            let pool = DarkPool::new(self.contract, &self.provider);
            let raw = tokio::time::timeout(LOOKUP_TIMEOUT, pool.reserved(trader, asset).call())
                .await
                .map_err(|_| {
                    SettlementError::Rpc(format!(
                        "balance oracle lookup timed out after {LOOKUP_TIMEOUT:?}"
                    ))
                })?
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;
            let balance = raw_to_decimal(raw, decimals)?;
            Ok((balance, 0i128))
        })
    }
}

/// Convert a raw on-chain token amount (`value * 10^decimals`) into whole-token
/// `Decimal` units. Fails closed when the amount or `10^decimals` exceeds
/// `Decimal`'s range rather than silently truncating a balance the solvency
/// check would then trust.
fn raw_to_decimal(raw: U256, decimals: u8) -> Result<Decimal, SettlementError> {
    let int = Decimal::from_str(&raw.to_string()).map_err(|e| {
        SettlementError::Parse(format!("reserved {raw} exceeds Decimal range: {e}"))
    })?;
    let mut divisor = Decimal::ONE;
    for _ in 0..decimals {
        divisor = divisor
            .checked_mul(Decimal::TEN)
            .ok_or_else(|| SettlementError::Parse(format!("10^{decimals} overflows Decimal")))?;
    }
    int.checked_div(divisor).ok_or_else(|| {
        SettlementError::Parse(format!("reserved {raw} / 10^{decimals} overflows Decimal"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insecure_dev_oracle_returns_fixed_billion() {
        let (bal, pos) = InsecureDevOracle
            .lookup(Address::ZERO, Address::ZERO, 18)
            .await
            .unwrap();
        assert_eq!(bal, Decimal::from(1_000_000_000u64));
        assert_eq!(pos, 0);
    }

    #[test]
    fn raw_to_decimal_18_decimals() {
        // 1.5 ETH at 18 decimals.
        let raw = U256::from(15u64) * U256::from(10u64).pow(U256::from(17u64));
        assert_eq!(raw_to_decimal(raw, 18).unwrap(), Decimal::new(15, 1));
    }

    #[test]
    fn raw_to_decimal_6_decimals() {
        // 1000.5 USDC at 6 decimals.
        let raw = U256::from(1_000_500_000u64);
        assert_eq!(raw_to_decimal(raw, 6).unwrap(), Decimal::new(10005, 1));
    }

    #[test]
    fn raw_to_decimal_zero() {
        assert_eq!(raw_to_decimal(U256::ZERO, 18).unwrap(), Decimal::ZERO);
    }

    #[test]
    fn raw_to_decimal_overflows_fail_closed() {
        // 10^40 raw far exceeds Decimal's 96-bit mantissa: must error, not wrap.
        let raw = U256::from(10u64).pow(U256::from(40u64));
        assert!(raw_to_decimal(raw, 18).is_err());
    }
}
