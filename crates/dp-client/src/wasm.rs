//! Browser bindings (`wasm-pack build --target web --features wasm
//! --no-default-features`). All inputs/outputs cross the JS boundary as
//! strings or byte buffers — no JsValue object decoding on the hot path.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rust_decimal::Decimal;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::commitment::{
    commit_native, derive_trader_id as inner_derive_trader_id, scalar_to_be_bytes,
    OrderCommitmentInput,
};
use crate::encrypt::{encrypt_order, parse_operator_pubkey_hex};
use crate::payload::OrderPayload;
use crate::submit::prepare_order;

fn js_err<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
pub fn prepare_order_wasm(operator_pubkey_hex: &str, order_json: &str) -> Result<JsValue, JsError> {
    let pk = parse_operator_pubkey_hex(operator_pubkey_hex).map_err(js_err)?;
    let payload: OrderPayload = serde_json::from_str(order_json).map_err(js_err)?;
    let sub = prepare_order(&pk, &payload).map_err(js_err)?;
    let v = serde_json::json!({
        "commitment": hex::encode(sub.commitment),
        "proof": hex::encode(&sub.proof),
        "encrypted_payload": B64.encode(&sub.encrypted_payload),
        "trader_id": hex::encode(sub.trader_id),
        "salt": hex::encode(sub.salt),
    });
    serde_wasm_bindgen_serialize(&v)
}

#[wasm_bindgen]
pub fn encrypt_order_wasm(pubkey_hex: &str, order_json: &str) -> Result<Vec<u8>, JsError> {
    let pk = parse_operator_pubkey_hex(pubkey_hex).map_err(js_err)?;
    let payload: OrderPayload = serde_json::from_str(order_json).map_err(js_err)?;
    encrypt_order(&pk, &payload).map_err(js_err)
}

#[wasm_bindgen]
pub fn compute_commitment_wasm(
    commitment_key: &str,
    side: u8,
    price: &str,
    size: &str,
    salt_hex: &str,
) -> Result<String, JsError> {
    let trader_id = inner_derive_trader_id(commitment_key.as_bytes());
    let price_d = Decimal::from_str(price).map_err(js_err)?;
    let size_d = Decimal::from_str(size).map_err(js_err)?;
    let stripped = salt_hex
        .strip_prefix("0x")
        .or_else(|| salt_hex.strip_prefix("0X"))
        .unwrap_or(salt_hex);
    let salt_bytes = hex::decode(stripped).map_err(js_err)?;
    if salt_bytes.len() != 32 {
        return Err(JsError::new(&format!(
            "salt must be exactly 32 bytes, got {}",
            salt_bytes.len()
        )));
    }
    let salt_fr = crate::commitment::bytes_to_scalar(&salt_bytes);
    let inp = OrderCommitmentInput::from_decimals(trader_id, side, price_d, size_d, salt_fr)
        .map_err(js_err)?;
    Ok(hex::encode(scalar_to_be_bytes(commit_native(&inp))))
}

#[wasm_bindgen]
pub fn derive_trader_id_wasm(commitment_key: &str) -> String {
    let f = inner_derive_trader_id(commitment_key.as_bytes());
    hex::encode(scalar_to_be_bytes(f))
}

// Avoid pulling in `serde-wasm-bindgen` as a separate dep — JSON-string the
// payload and parse on the JS side.
fn serde_wasm_bindgen_serialize(v: &serde_json::Value) -> Result<JsValue, JsError> {
    let s = serde_json::to_string(v).map_err(js_err)?;
    Ok(JsValue::from_str(&s))
}
