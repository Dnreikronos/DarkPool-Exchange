//! Deterministic scalar encoding for `rust_decimal::Decimal` values.
//!
//! Strategy: multiply by `SCALE_FACTOR` (= 1e8), round-trip through `i128`,
//! then convert to a BN254 scalar field element. Range-bounded to fit in 60
//! bits so an in-circuit range proof is cheap.

use ark_bn254::Fr;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

pub const DECIMAL_SCALE: u32 = 8;
pub const SCALE_FACTOR_I128: i128 = 100_000_000;
/// Hard cap for any encoded decimal: ~ 2^60.
pub const MAX_ENCODED: i128 = 1 << 60;

#[derive(Debug, thiserror::Error)]
pub enum EncodingError {
    #[error("decimal {0} is negative; cannot encode as field scalar")]
    Negative(Decimal),
    #[error("decimal {0} exceeds 2^60 after scaling")]
    Overflow(Decimal),
    #[error("decimal {0} loses precision beyond {DECIMAL_SCALE} dp")]
    Precision(Decimal),
    #[error("field element encoding must be at most 32 bytes, got {0}")]
    FieldElementTooLong(usize),
    #[error("field element encoding must be exactly 32 bytes, got {actual}")]
    InvalidFieldElementLength { actual: usize },
    #[error("field element encoding is not canonical for BN254 Fr")]
    NonCanonicalFieldElement,
}

/// Convert a non-negative decimal to a BN254 scalar.
pub fn decimal_to_scalar(d: Decimal) -> Result<Fr, EncodingError> {
    if d.is_sign_negative() {
        return Err(EncodingError::Negative(d));
    }
    let scaled = d
        .checked_mul(Decimal::from(SCALE_FACTOR_I128))
        .ok_or(EncodingError::Overflow(d))?;
    if scaled.scale() > 0 && !scaled.fract().is_zero() {
        return Err(EncodingError::Precision(d));
    }
    let int = scaled.trunc().to_i128().ok_or(EncodingError::Overflow(d))?;
    if !(0..MAX_ENCODED).contains(&int) {
        return Err(EncodingError::Overflow(d));
    }
    debug_assert!(
        int >= 0,
        "negative int should have been caught by sign check"
    );
    Ok(Fr::from(int as u128))
}

/// Best-effort decode (only used in tests / debugging). Lossy for values
/// outside the encoder's accepted range.
#[cfg(test)]
pub fn scalar_to_decimal(f: Fr) -> Decimal {
    use ark_ff::{BigInteger, PrimeField};
    let bigint = f.into_bigint();
    let bytes = bigint.to_bytes_le();
    let mut buf = [0u8; 16];
    let take = bytes.len().min(16);
    buf[..take].copy_from_slice(&bytes[..take]);
    let raw = u128::from_le_bytes(buf) as i128;
    Decimal::new(raw as i64, DECIMAL_SCALE)
}

/// Convert a BN254 scalar to a big-endian 32-byte array suitable for
/// constructing an `alloy_primitives::U256`.
pub fn fr_to_bytes32(f: Fr) -> [u8; 32] {
    use ark_ff::{BigInteger, PrimeField};
    let le = f.into_bigint().to_bytes_le();
    let mut be = [0u8; 32];
    for (i, b) in le.iter().enumerate().take(32) {
        be[31 - i] = *b;
    }
    be
}

/// i128 → Fr for signed values (e.g. position). Caller must keep |x| < 2^60.
pub fn signed_to_scalar(x: i128) -> Result<Fr, EncodingError> {
    if x.abs() >= MAX_ENCODED {
        return Err(EncodingError::Overflow(Decimal::from(x)));
    }
    if x >= 0 {
        Ok(Fr::from(x as u128))
    } else {
        Ok(-Fr::from((-x) as u128))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_integers() {
        for v in [0i64, 1, 100, 1_000_000] {
            let d = Decimal::from(v);
            let f = decimal_to_scalar(d).unwrap();
            let back = scalar_to_decimal(f);
            assert_eq!(back, d);
        }
    }

    #[test]
    fn rejects_negative() {
        let d = Decimal::new(-1, 0);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(EncodingError::Negative(_))
        ));
    }

    #[test]
    fn rejects_overflow() {
        let d = Decimal::from(i64::MAX);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(EncodingError::Overflow(_))
        ));
    }

    #[test]
    fn rejects_excess_precision() {
        // 1e-9 cannot be represented at 8dp.
        let d = Decimal::new(1, 9);
        assert!(matches!(
            decimal_to_scalar(d),
            Err(EncodingError::Precision(_))
        ));
    }

    #[test]
    fn signed_round_trip() {
        use ark_std::Zero;
        for v in [-1000i128, -1, 0, 1, 1000] {
            let f = signed_to_scalar(v).unwrap();
            if v == 0 {
                assert!(f.is_zero());
            }
        }
    }
}
