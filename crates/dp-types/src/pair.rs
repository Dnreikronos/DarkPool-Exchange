use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DarkPoolError;

/// Validated trading pair identifier in canonical form.
///
/// Canonical form rules:
/// - non-empty after trimming whitespace
/// - uppercase ASCII
/// - characters limited to `A-Z`, `0-9`, and the separators `/`, `-`, `_`
/// - at least one separator that splits the string into a non-empty
///   base and a non-empty quote
///
/// `Pair::parse` accepts both `BTC/USD` and `btc-usd` and canonicalises
/// them; downstream code should compare against `Pair::as_str()` (or the
/// `String` it wraps) rather than the raw user input.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pair(String);

impl Pair {
    pub fn parse(input: &str) -> Result<Self, DarkPoolError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(DarkPoolError::PairRequired);
        }
        let upper = trimmed.to_ascii_uppercase();
        let sep_idx = upper
            .find(['/', '-', '_'])
            .ok_or_else(|| DarkPoolError::InvalidPair(input.to_string()))?;
        let (base, rest) = upper.split_at(sep_idx);
        let quote = &rest[1..];
        if base.is_empty() || quote.is_empty() {
            return Err(DarkPoolError::InvalidPair(input.to_string()));
        }
        for c in upper.chars() {
            let ok = c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_';
            if !ok {
                return Err(DarkPoolError::InvalidPair(input.to_string()));
            }
        }
        Ok(Self(upper))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Pair {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Pair> for String {
    fn from(p: Pair) -> Self {
        p.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonicalises_case() {
        assert_eq!(Pair::parse("btc/usd").unwrap().as_str(), "BTC/USD");
        assert_eq!(Pair::parse("Eth-Usdc").unwrap().as_str(), "ETH-USDC");
    }

    #[test]
    fn parse_trims_whitespace() {
        assert_eq!(Pair::parse("  ETH/USDC  ").unwrap().as_str(), "ETH/USDC");
    }

    #[test]
    fn parse_rejects_empty() {
        assert_eq!(Pair::parse("").unwrap_err(), DarkPoolError::PairRequired);
        assert_eq!(Pair::parse("   ").unwrap_err(), DarkPoolError::PairRequired);
    }

    #[test]
    fn parse_rejects_missing_separator() {
        assert!(matches!(
            Pair::parse("ETHUSDC"),
            Err(DarkPoolError::InvalidPair(_))
        ));
    }

    #[test]
    fn parse_rejects_empty_side() {
        assert!(matches!(
            Pair::parse("/USDC"),
            Err(DarkPoolError::InvalidPair(_))
        ));
        assert!(matches!(
            Pair::parse("ETH/"),
            Err(DarkPoolError::InvalidPair(_))
        ));
    }

    #[test]
    fn parse_rejects_invalid_chars() {
        assert!(matches!(
            Pair::parse("ETH/USDC$"),
            Err(DarkPoolError::InvalidPair(_))
        ));
        assert!(matches!(
            Pair::parse("ETH USDC"),
            Err(DarkPoolError::InvalidPair(_))
        ));
    }

    #[test]
    fn parse_accepts_dash_and_underscore() {
        assert_eq!(Pair::parse("eth-usdc").unwrap().as_str(), "ETH-USDC");
        assert_eq!(Pair::parse("eth_usdc").unwrap().as_str(), "ETH_USDC");
    }

    #[test]
    fn into_string_returns_canonical_form() {
        let p = Pair::parse("btc/usd").unwrap();
        assert_eq!(p.into_string(), "BTC/USD");
    }

    #[test]
    fn display_matches_as_str() {
        let p = Pair::parse("eth/usdc").unwrap();
        assert_eq!(format!("{}", p), "ETH/USDC");
        assert_eq!(p.to_string(), "ETH/USDC");
    }

    #[test]
    fn as_ref_str_returns_canonical_form() {
        let p = Pair::parse("eth-usdc").unwrap();
        let s: &str = p.as_ref();
        assert_eq!(s, "ETH-USDC");
    }

    #[test]
    fn from_pair_into_string() {
        let p = Pair::parse("eth_usdc").unwrap();
        let s: String = String::from(p);
        assert_eq!(s, "ETH_USDC");
    }

    #[test]
    fn parse_rejects_separator_only() {
        assert!(matches!(
            Pair::parse("/"),
            Err(DarkPoolError::InvalidPair(_))
        ));
        assert!(matches!(
            Pair::parse("-"),
            Err(DarkPoolError::InvalidPair(_))
        ));
        assert!(matches!(
            Pair::parse("_"),
            Err(DarkPoolError::InvalidPair(_))
        ));
    }

    #[test]
    fn hash_and_equality_match_canonical_form() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Pair::parse("eth/usdc").unwrap());
        assert!(set.contains(&Pair::parse("ETH/USDC").unwrap()));
        assert!(set.contains(&Pair::parse("  eth/usdc  ").unwrap()));
    }
}
