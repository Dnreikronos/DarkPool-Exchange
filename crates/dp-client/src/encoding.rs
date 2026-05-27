//! Decimal → BN254 scalar encoder. Mirrors `dp_zk::encoding::decimal_to_scalar`
//! byte-for-byte; the interop test in `tests/interop.rs` enforces this.

use ark_bn254::Fr;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::error::ClientError;

pub const DECIMAL_SCALE: u32 = 8;
pub const SCALE_FACTOR_I128: i128 = 100_000_000;
pub const MAX_ENCODED: i128 = 1 << 60;

pub fn decimal_to_scalar(d: Decimal) -> Result<Fr, ClientError> {
    if d.is_sign_negative() {
        return Err(ClientError::Encoding(format!("decimal {d} is negative")));
    }
    let scaled = d
        .checked_mul(Decimal::from(SCALE_FACTOR_I128))
        .ok_or_else(|| ClientError::Encoding(format!("decimal {d} overflows after scaling")))?;
    if scaled.scale() > 0 && !scaled.fract().is_zero() {
        return Err(ClientError::Encoding(format!(
            "decimal {d} loses precision beyond {DECIMAL_SCALE} dp"
        )));
    }
    let int = scaled
        .trunc()
        .to_i128()
        .ok_or_else(|| ClientError::Encoding(format!("decimal {d} overflows to_i128")))?;
    if !(0..MAX_ENCODED).contains(&int) {
        return Err(ClientError::Encoding(format!("decimal {d} out of range")));
    }
    Ok(Fr::from(int as u128))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_negative() {
        let d = Decimal::new(-1, 0);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(ClientError::Encoding(_))
        ));
    }

    #[test]
    fn rejects_excess_precision() {
        let d = Decimal::new(1, 9);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(ClientError::Encoding(_))
        ));
    }

    #[test]
    fn rejects_overflow() {
        let d = Decimal::from(i64::MAX);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(ClientError::Encoding(_))
        ));
    }

    #[test]
    fn accepts_zero() {
        decimal_to_scalar(Decimal::ZERO).unwrap();
    }
}
