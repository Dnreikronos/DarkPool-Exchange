use alloy_primitives::{FixedBytes, U256};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::abi::SolMatch;
use crate::{SettlementError, SettlementMatch};

/// Wei per unit of the circuit's 1e8 fixed-point domain (`1e18 / 1e8`). The
/// #209 on-chain settlement binding folds price/size after dividing by this and
/// reverts ("match precision") on any wei amount it does not evenly divide.
const WEI_TO_CIRCUIT_SCALE: u64 = 10_000_000_000;

/// Convert an off-chain [`SettlementMatch`] into the on-chain `Match` struct.
/// Amounts are scaled to wei via [`decimal_to_wei`]; the on-chain settlement
/// binding (#209) re-derives the circuit-domain values from these.
///
/// Rejects price/size carrying finer than 1e8 precision. The circuit proves —
/// and `settleAuction` recomputes the binding — over 1e8 fixed-point values, so
/// a sub-1e8 amount could never have been proved and would revert on-chain.
/// Failing here surfaces a clear error before submit rather than an opaque
/// revert that strands the auction. (Quantizing amounts at the source is #211.)
pub fn settlement_match_to_sol(m: &SettlementMatch) -> Result<SolMatch, SettlementError> {
    let price = decimal_to_wei(m.price)?;
    let size = decimal_to_wei(m.size)?;
    let scale = U256::from(WEI_TO_CIRCUIT_SCALE);
    if price % scale != U256::ZERO || size % scale != U256::ZERO {
        return Err(SettlementError::SubCircuitPrecision);
    }
    Ok(SolMatch {
        bidOrderId: uuid_to_bytes32(m.bid_order_id),
        askOrderId: uuid_to_bytes32(m.ask_order_id),
        bidTrader: m.bid_trader,
        askTrader: m.ask_trader,
        baseToken: m.base_token,
        quoteToken: m.quote_token,
        price,
        size,
    })
}

pub fn uuid_to_bytes32(id: Uuid) -> FixedBytes<32> {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(id.as_bytes());
    FixedBytes(out)
}

pub fn bytes32_to_uuid(b: FixedBytes<32>) -> Result<Uuid, SettlementError> {
    for i in 0..16 {
        if b[i] != 0 {
            return Err(SettlementError::InvalidBytes32);
        }
    }
    Ok(Uuid::from_bytes(b[16..].try_into().unwrap()))
}

pub fn decimal_to_wei(d: Decimal) -> Result<U256, SettlementError> {
    if d.is_sign_negative() {
        return Err(SettlementError::NegativeAmount);
    }
    let wei_factor = Decimal::from(10u64.pow(18));
    let scaled = d * wei_factor;
    if scaled != scaled.trunc() {
        return Err(SettlementError::PrecisionLoss);
    }
    let s = scaled.trunc().to_string();
    U256::from_str_radix(&s, 10).map_err(|e| SettlementError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_roundtrip() {
        let id = Uuid::new_v4();
        let b = uuid_to_bytes32(id);
        let back = bytes32_to_uuid(b).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn invalid_bytes32() {
        let mut b = FixedBytes([0u8; 32]);
        b[0] = 0xFF;
        assert!(bytes32_to_uuid(b).is_err());
    }

    #[test]
    fn decimal_to_wei_zero() {
        let w = decimal_to_wei(Decimal::ZERO).unwrap();
        assert_eq!(w, U256::ZERO);
    }

    #[test]
    fn decimal_to_wei_fractional() {
        let d = Decimal::new(15, 1); // 1.5
        let w = decimal_to_wei(d).unwrap();
        let expected = U256::from(15u64) * U256::from(10u64).pow(U256::from(17u64));
        assert_eq!(w, expected);
    }

    #[test]
    fn decimal_to_wei_negative() {
        let d = Decimal::new(-1, 0);
        assert!(matches!(
            decimal_to_wei(d),
            Err(SettlementError::NegativeAmount)
        ));
    }

    #[test]
    fn decimal_to_wei_too_many_decimals() {
        let d = Decimal::new(1, 19); // 1e-19 — more than 18 decimals
        assert!(matches!(
            decimal_to_wei(d),
            Err(SettlementError::PrecisionLoss)
        ));
    }

    /// The #209 settlement binding hashes price/size in the circuit's 1e8
    /// fixed-point domain, recovered on-chain by dividing the wei amount by
    /// `WEI_TO_CIRCUIT_SCALE` (1e10) in `DarkPool.settleAuction`. That constant
    /// is exactly `decimal_to_wei`'s 1e18 scale divided by the circuit's 1e8
    /// scale. Lock the relationship so any change to the wei scaling (#211)
    /// trips here rather than silently desyncing the on-chain binding from the
    /// proof.
    #[test]
    fn decimal_to_wei_bridges_circuit_scale() {
        let scale_1e8 = Decimal::from(100_000_000u64);
        let bridge = U256::from(10u64).pow(U256::from(10u64)); // WEI_TO_CIRCUIT_SCALE
        for d in [
            Decimal::from(100),
            Decimal::new(1005, 1),       // 100.5
            Decimal::new(12_345_678, 8), // 0.12345678 (max circuit precision)
        ] {
            let wei = decimal_to_wei(d).unwrap();
            let circuit_1e8 =
                U256::from_str_radix(&(d * scale_1e8).trunc().to_string(), 10).unwrap();
            assert_eq!(
                wei,
                circuit_1e8 * bridge,
                "wei must equal the 1e8 circuit value times WEI_TO_CIRCUIT_SCALE"
            );
        }
    }

    fn sample_match(price: Decimal, size: Decimal) -> SettlementMatch {
        use alloy_primitives::Address;
        SettlementMatch {
            bid_order_id: Uuid::nil(),
            ask_order_id: Uuid::nil(),
            bid_trader: Address::ZERO,
            ask_trader: Address::ZERO,
            base_token: Address::ZERO,
            quote_token: Address::ZERO,
            price,
            size,
        }
    }

    /// The #209 on-chain binding rejects price/size finer than 1e8 precision
    /// (wei not divisible by WEI_TO_CIRCUIT_SCALE). The off-chain converter must
    /// reject the same, so the operator fails fast instead of stranding the
    /// auction on an on-chain "match precision" revert.
    #[test]
    fn settlement_match_to_sol_rejects_sub_1e8_precision() {
        // 100.000000001 has 9 dp → its wei value carries sub-1e8 precision.
        let m = sample_match(Decimal::new(100_000_000_001, 9), Decimal::from(10));
        assert!(matches!(
            settlement_match_to_sol(&m),
            Err(SettlementError::SubCircuitPrecision)
        ));
    }

    /// 8 dp is exactly the circuit's fixed-point domain, so it converts cleanly.
    #[test]
    fn settlement_match_to_sol_accepts_max_circuit_precision() {
        let m = sample_match(Decimal::new(12_345_678, 8), Decimal::from(10));
        let sol = settlement_match_to_sol(&m).unwrap();
        let bridge = U256::from(10u64).pow(U256::from(10u64));
        assert_eq!(sol.price, U256::from(12_345_678u64) * bridge);
    }
}
