use alloy_primitives::{FixedBytes, U256};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::abi::SolMatch;
use crate::{SettlementError, SettlementMatch};

#[cfg(feature = "hypernova")]
use alloy_primitives::{keccak256, B256};

/// Mirror of `keccak256(abi.encode(auctionId, matches))` from
/// `DarkPool.settleAuction`. The operator computes this off-chain at
/// session-submission time and the contract re-derives it inside
/// `settleAuction` to reject any post-session match substitution.
pub fn settlement_match_to_sol(m: &SettlementMatch) -> Result<SolMatch, SettlementError> {
    Ok(SolMatch {
        bidOrderId: uuid_to_bytes32(m.bid_order_id),
        askOrderId: uuid_to_bytes32(m.ask_order_id),
        bidTrader: m.bid_trader,
        askTrader: m.ask_trader,
        baseToken: m.base_token,
        quoteToken: m.quote_token,
        price: decimal_to_wei(m.price)?,
        size: decimal_to_wei(m.size)?,
    })
}

#[cfg(feature = "hypernova")]
pub fn compute_matches_hash(
    auction_id: Uuid,
    matches: &[SettlementMatch],
) -> Result<B256, SettlementError> {
    use alloy_sol_types::SolValue;

    let sol_matches: Vec<SolMatch> = matches
        .iter()
        .map(settlement_match_to_sol)
        .collect::<Result<Vec<_>, SettlementError>>()?;

    let auction_id_bytes = uuid_to_bytes32(auction_id);
    let encoded = (auction_id_bytes, sol_matches).abi_encode_params();
    Ok(keccak256(&encoded))
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
}
