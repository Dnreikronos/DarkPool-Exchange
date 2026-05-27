// Decimal serde wrapper that adapts to the serializer.
//
// rust_decimal's default `serde-with-str` impl calls `deserialize_any`, which
// non-self-describing formats like bincode reject. This module emits a string
// for human-readable formats (JSON, YAML) and a fixed 16-byte array for binary
// formats (bincode), so persisted event logs round-trip cleanly.

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    if s.is_human_readable() {
        d.to_string().serialize(s)
    } else {
        d.serialize().serialize(s)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
    if d.is_human_readable() {
        let s = String::deserialize(d)?;
        Decimal::from_str(&s).map_err(serde::de::Error::custom)
    } else {
        let bytes: [u8; 16] = Deserialize::deserialize(d)?;
        Ok(Decimal::deserialize(bytes))
    }
}
