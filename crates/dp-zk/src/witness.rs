//! Witness types shared between the engine, the aggregator subprocess
//! wire-format, and the circuit. Serializable.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::encoding::{decimal_to_scalar, signed_to_scalar, EncodingError};
use ark_bn254::Fr;

/// One leg (bid or ask) of a matched pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderLegWitness {
    /// Hex-encoded 32-byte trader id (BE bytes of `poseidon(commitment_key_scalar)`).
    pub trader_id: String,
    /// Hex-encoded 32-byte commitment salt.
    pub salt: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub balance: Decimal,
    /// Signed integer (string-serialized to keep precision).
    pub position: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    /// Original order size (used in commitment binding, distinct from match
    /// fill size for partial fills).
    #[serde(with = "rust_decimal::serde::str")]
    pub order_size: Decimal,
    /// 0 = bid (buy), 1 = ask (sell).
    pub side: u8,
    /// Trader's commitment-key string. Bound into the circuit so the
    /// prover proves `trader_id == poseidon(commitment_key_scalar)`.
    #[serde(default)]
    pub commitment_key: String,
}

/// One matched pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchWitness {
    pub bid: OrderLegWitness,
    pub ask: OrderLegWitness,
}

/// Per-batch policy bound into proof public inputs so the verifier ties a
/// proof to a concrete policy version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    #[serde(with = "rust_decimal::serde::str")]
    pub min_size: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub min_price: Decimal,
    /// Signed; serialized as string.
    pub position_limit: String,
}

/// Default policy used when subprocess input omits one.
pub const DEFAULT_POLICY: PolicyDefault = PolicyDefault;

pub struct PolicyDefault;

impl PolicyDefault {
    pub fn into_policy(&self) -> Policy {
        Policy {
            min_size: Decimal::new(1, 8),  // 1e-8
            min_price: Decimal::new(1, 8), // 1e-8
            // 2^58 — large enough to be effectively unbounded, small
            // enough to fit the 60-bit encoder cap with margin.
            position_limit: (1i128 << 58).to_string(),
        }
    }
}

/// Full per-batch witness, mirroring the JSON wire format consumed by
/// `dp-zk-cli`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchWitness {
    pub batch_id: Uuid,
    pub auction_id: Uuid,
    pub matches: Vec<MatchWitness>,
    pub policy: Policy,
}

impl BatchWitness {
    pub fn empty(batch_id: Uuid, auction_id: Uuid) -> Self {
        Self {
            batch_id,
            auction_id,
            matches: Vec::new(),
            policy: DEFAULT_POLICY.into_policy(),
        }
    }
}

/// Helpers for converting witness fields to scalars.
impl OrderLegWitness {
    pub fn trader_id_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(self.trader_id.trim_start_matches("0x"))
    }
    pub fn salt_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(self.salt.trim_start_matches("0x"))
    }
    pub fn position_i128(&self) -> Result<i128, std::num::ParseIntError> {
        self.position.parse::<i128>()
    }
    pub fn balance_scalar(&self) -> Result<Fr, EncodingError> {
        decimal_to_scalar(self.balance)
    }
    pub fn limit_price_scalar(&self) -> Result<Fr, EncodingError> {
        decimal_to_scalar(self.limit_price)
    }
    pub fn order_size_scalar(&self) -> Result<Fr, EncodingError> {
        decimal_to_scalar(self.order_size)
    }
    pub fn position_scalar(&self) -> Result<Fr, EncodingError> {
        let p = self
            .position_i128()
            .map_err(|_| EncodingError::Overflow(Decimal::ZERO))?;
        signed_to_scalar(p)
    }
    /// Map the commitment-key string to an Fr via `from_be_bytes_mod_order`.
    /// Mirrors `pedersen::derive_trader_id`'s input projection.
    pub fn commitment_key_scalar(&self) -> Fr {
        crate::pedersen::bytes_to_scalar(self.commitment_key.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let w = BatchWitness {
            batch_id: Uuid::nil(),
            auction_id: Uuid::nil(),
            matches: vec![MatchWitness {
                bid: OrderLegWitness {
                    trader_id: "00".repeat(32),
                    salt: "11".repeat(32),
                    balance: Decimal::from(1000),
                    position: "0".into(),
                    limit_price: Decimal::from(100),
                    order_size: Decimal::from(10),
                    side: 0,
                    commitment_key: "bid_key".into(),
                },
                ask: OrderLegWitness {
                    trader_id: "22".repeat(32),
                    salt: "33".repeat(32),
                    balance: Decimal::from(2000),
                    position: "0".into(),
                    limit_price: Decimal::from(99),
                    order_size: Decimal::from(10),
                    side: 1,
                    commitment_key: "ask_key".into(),
                },
            }],
            policy: DEFAULT_POLICY.into_policy(),
        };
        let s = serde_json::to_string(&w).unwrap();
        let b: BatchWitness = serde_json::from_str(&s).unwrap();
        assert_eq!(b.matches.len(), 1);
        assert_eq!(b.matches[0].bid.side, 0);
        assert_eq!(b.matches[0].ask.side, 1);
    }
}
