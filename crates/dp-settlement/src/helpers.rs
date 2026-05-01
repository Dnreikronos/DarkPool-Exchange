use alloy_primitives::{FixedBytes, U256};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::SettlementError;

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
