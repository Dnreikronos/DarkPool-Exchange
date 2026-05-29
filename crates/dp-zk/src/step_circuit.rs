use std::borrow::Borrow;

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::constraints::PoseidonSpongeVar;
use ark_ff::{One, Zero};
use ark_r1cs_std::alloc::{AllocVar, AllocationMode};
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::ToBitsGadget;
use ark_r1cs_std::prelude::*;
use ark_relations::gr1cs::{ConstraintSystemRef, Namespace, SynthesisError};
use folding_schemes::{frontend::FCircuit, Error};

use crate::encoding::SCALE_FACTOR_I128;
use crate::pedersen::{bytes_to_scalar, poseidon_config};
use crate::witness::BatchWitness;
use crate::ZkError;

const SIZE_BITS: usize = 60;
const SOLVENCY_DIFF_BITS: usize = 120;

// ─── Native data types ───────────────────────────────────────────────────────

/// Per-match native witness: 19 Fr fields.
#[derive(Clone, Debug)]
pub struct CircuitMatchNative {
    pub bid_trader: Fr,
    pub bid_salt: Fr,
    pub bid_limit_price: Fr,
    pub bid_side: Fr,
    pub bid_balance: Fr,
    pub bid_position: Fr,
    pub bid_trader_addr: Fr,
    pub bid_order_size: Fr,

    pub ask_trader: Fr,
    pub ask_salt: Fr,
    pub ask_limit_price: Fr,
    pub ask_side: Fr,
    pub ask_balance: Fr,
    pub ask_position: Fr,
    pub ask_trader_addr: Fr,
    pub ask_order_size: Fr,

    pub match_price: Fr,
    pub match_size: Fr,
    pub is_active: Fr,
}

impl Default for CircuitMatchNative {
    fn default() -> Self {
        let zero = Fr::zero();
        let one = Fr::one();
        Self {
            bid_trader: zero,
            bid_salt: zero,
            bid_limit_price: zero,
            bid_side: zero,
            bid_balance: zero,
            bid_position: zero,
            bid_trader_addr: zero,
            bid_order_size: zero,
            ask_trader: zero,
            ask_salt: zero,
            ask_limit_price: zero,
            ask_side: one, // ask side = 1 for inactive rows (valid bit)
            ask_balance: zero,
            ask_position: zero,
            ask_trader_addr: zero,
            ask_order_size: zero,
            match_price: zero,
            match_size: zero,
            is_active: zero,
        }
    }
}

/// External inputs for one IVC step: policy fields + all match rows.
#[derive(Clone, Debug)]
pub struct AuctionExternalInputs {
    pub min_size: Fr,
    pub min_price: Fr,
    pub position_limit: Fr,
    /// The single uniform clearing price for this auction round. Every active
    /// match row must settle at this price (#163). Derived from the first
    /// match in [`from_witness`]; the honest engine applies one volume-
    /// maximizing price to every match, so all rows already equal it.
    pub clearing_price: Fr,
    pub matches: Vec<CircuitMatchNative>,
}

impl Default for AuctionExternalInputs {
    fn default() -> Self {
        Self {
            min_size: Fr::zero(),
            min_price: Fr::zero(),
            position_limit: Fr::zero(),
            clearing_price: Fr::zero(),
            matches: Vec::new(),
        }
    }
}

impl AuctionExternalInputs {
    /// Convert a [`BatchWitness`] + per-match prices/sizes into
    /// [`AuctionExternalInputs`], padding inactive rows to `batch_size`.
    pub fn from_witness(
        witness: &BatchWitness,
        match_prices: &[rust_decimal::Decimal],
        match_sizes: &[rust_decimal::Decimal],
        batch_size: usize,
    ) -> Result<Self, ZkError> {
        use crate::encoding::{decimal_to_scalar, signed_to_scalar};

        if witness.matches.len() > batch_size {
            return Err(ZkError::Witness(format!(
                "{} matches > circuit batch_size {}",
                witness.matches.len(),
                batch_size
            )));
        }
        if witness.matches.len() != match_prices.len() || match_prices.len() != match_sizes.len() {
            return Err(ZkError::Witness(
                "match_prices/match_sizes length mismatch".into(),
            ));
        }

        let min_size = decimal_to_scalar(witness.policy.min_size)?;
        let min_price = decimal_to_scalar(witness.policy.min_price)?;
        let pos_limit_i: i128 = witness
            .policy
            .position_limit
            .parse()
            .map_err(|_| ZkError::Witness("policy.position_limit invalid".into()))?;
        let position_limit = signed_to_scalar(pos_limit_i)?;

        let mut matches = Vec::with_capacity(batch_size);

        for (i, m) in witness.matches.iter().enumerate() {
            let bid_trader = bytes_to_scalar(
                &m.bid
                    .trader_id_bytes()
                    .map_err(|e| ZkError::Witness(format!("bid trader_id: {e}")))?,
            );
            let bid_salt = bytes_to_scalar(
                &m.bid
                    .salt_bytes()
                    .map_err(|e| ZkError::Witness(format!("bid salt: {e}")))?,
            );
            let ask_trader = bytes_to_scalar(
                &m.ask
                    .trader_id_bytes()
                    .map_err(|e| ZkError::Witness(format!("ask trader_id: {e}")))?,
            );
            let ask_salt = bytes_to_scalar(
                &m.ask
                    .salt_bytes()
                    .map_err(|e| ZkError::Witness(format!("ask salt: {e}")))?,
            );

            matches.push(CircuitMatchNative {
                bid_trader,
                bid_salt,
                bid_limit_price: m.bid.limit_price_scalar()?,
                bid_side: Fr::from(m.bid.side as u64),
                bid_balance: m.bid.balance_scalar()?,
                bid_position: m.bid.position_scalar()?,
                bid_trader_addr: m
                    .bid
                    .trader_addr_scalar()
                    .map_err(|e| ZkError::Witness(format!("bid trader_addr: {e}")))?,
                bid_order_size: m.bid.order_size_scalar()?,
                ask_trader,
                ask_salt,
                ask_limit_price: m.ask.limit_price_scalar()?,
                ask_side: Fr::from(m.ask.side as u64),
                ask_balance: m.ask.balance_scalar()?,
                ask_position: m.ask.position_scalar()?,
                ask_trader_addr: m
                    .ask
                    .trader_addr_scalar()
                    .map_err(|e| ZkError::Witness(format!("ask trader_addr: {e}")))?,
                ask_order_size: m.ask.order_size_scalar()?,
                match_price: decimal_to_scalar(match_prices[i])?,
                match_size: decimal_to_scalar(match_sizes[i])?,
                is_active: Fr::one(),
            });
        }

        // Pad with inactive rows.
        while matches.len() < batch_size {
            matches.push(CircuitMatchNative::default());
        }

        // The clearing price is the single price every match settles at. The
        // engine applies one clearing price to all matches, so the first
        // match's price is that value; the circuit then enforces every active
        // row equals it.
        let clearing_price = match match_prices.first() {
            Some(p) => decimal_to_scalar(*p)?,
            None => Fr::zero(),
        };

        Ok(Self {
            min_size,
            min_price,
            position_limit,
            clearing_price,
            matches,
        })
    }
}

// ─── In-circuit (variable) data types ────────────────────────────────────────

/// In-circuit version of [`CircuitMatchNative`].
#[derive(Clone, Debug)]
pub struct CircuitMatchVar {
    pub bid_trader: FpVar<Fr>,
    pub bid_salt: FpVar<Fr>,
    pub bid_limit_price: FpVar<Fr>,
    pub bid_side: FpVar<Fr>,
    pub bid_balance: FpVar<Fr>,
    pub bid_position: FpVar<Fr>,
    pub bid_trader_addr: FpVar<Fr>,
    pub bid_order_size: FpVar<Fr>,

    pub ask_trader: FpVar<Fr>,
    pub ask_salt: FpVar<Fr>,
    pub ask_limit_price: FpVar<Fr>,
    pub ask_side: FpVar<Fr>,
    pub ask_balance: FpVar<Fr>,
    pub ask_position: FpVar<Fr>,
    pub ask_trader_addr: FpVar<Fr>,
    pub ask_order_size: FpVar<Fr>,

    pub match_price: FpVar<Fr>,
    pub match_size: FpVar<Fr>,
    pub is_active: FpVar<Fr>,
}

/// In-circuit version of [`AuctionExternalInputs`].
#[derive(Clone, Debug)]
pub struct AuctionExternalInputsVar {
    pub min_size: FpVar<Fr>,
    pub min_price: FpVar<Fr>,
    pub position_limit: FpVar<Fr>,
    pub clearing_price: FpVar<Fr>,
    pub matches: Vec<CircuitMatchVar>,
}

impl AllocVar<AuctionExternalInputs, Fr> for AuctionExternalInputsVar {
    fn new_variable<T: Borrow<AuctionExternalInputs>>(
        cs: impl Into<Namespace<Fr>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();

        let native = f().map(|v| v.borrow().clone()).unwrap_or_default();

        let min_size = FpVar::new_variable(cs.clone(), || Ok(native.min_size), mode)?;
        let min_price = FpVar::new_variable(cs.clone(), || Ok(native.min_price), mode)?;
        let position_limit = FpVar::new_variable(cs.clone(), || Ok(native.position_limit), mode)?;
        let clearing_price = FpVar::new_variable(cs.clone(), || Ok(native.clearing_price), mode)?;

        let mut matches = Vec::with_capacity(native.matches.len());
        for cm in &native.matches {
            matches.push(CircuitMatchVar {
                bid_trader: FpVar::new_variable(cs.clone(), || Ok(cm.bid_trader), mode)?,
                bid_salt: FpVar::new_variable(cs.clone(), || Ok(cm.bid_salt), mode)?,
                bid_limit_price: FpVar::new_variable(cs.clone(), || Ok(cm.bid_limit_price), mode)?,
                bid_side: FpVar::new_variable(cs.clone(), || Ok(cm.bid_side), mode)?,
                bid_balance: FpVar::new_variable(cs.clone(), || Ok(cm.bid_balance), mode)?,
                bid_position: FpVar::new_variable(cs.clone(), || Ok(cm.bid_position), mode)?,
                bid_trader_addr: FpVar::new_variable(cs.clone(), || Ok(cm.bid_trader_addr), mode)?,
                bid_order_size: FpVar::new_variable(cs.clone(), || Ok(cm.bid_order_size), mode)?,
                ask_trader: FpVar::new_variable(cs.clone(), || Ok(cm.ask_trader), mode)?,
                ask_salt: FpVar::new_variable(cs.clone(), || Ok(cm.ask_salt), mode)?,
                ask_limit_price: FpVar::new_variable(cs.clone(), || Ok(cm.ask_limit_price), mode)?,
                ask_side: FpVar::new_variable(cs.clone(), || Ok(cm.ask_side), mode)?,
                ask_balance: FpVar::new_variable(cs.clone(), || Ok(cm.ask_balance), mode)?,
                ask_position: FpVar::new_variable(cs.clone(), || Ok(cm.ask_position), mode)?,
                ask_trader_addr: FpVar::new_variable(cs.clone(), || Ok(cm.ask_trader_addr), mode)?,
                ask_order_size: FpVar::new_variable(cs.clone(), || Ok(cm.ask_order_size), mode)?,
                match_price: FpVar::new_variable(cs.clone(), || Ok(cm.match_price), mode)?,
                match_size: FpVar::new_variable(cs.clone(), || Ok(cm.match_size), mode)?,
                is_active: FpVar::new_variable(cs.clone(), || Ok(cm.is_active), mode)?,
            });
        }

        Ok(Self {
            min_size,
            min_price,
            position_limit,
            clearing_price,
            matches,
        })
    }
}

// ─── Step circuit ─────────────────────────────────────────────────────────────

/// IVC step circuit for HyperNova.
///
/// IVC state: `z_i = [state_hash, round_nonce, policy_hash]` (3 Fr elements).
///
/// Each step folds one auction batch, enforcing the same 9 constraint families
/// as `BatchProofCircuit`, then updates the state:
/// ```text
/// policy_hash_computed = poseidon(min_size, min_price, position_limit)
/// // checked == z_i[2]
/// new_state_hash = poseidon(z_i[0], commitments_root, notionals_root, active_count)
/// z_{i+1} = [new_state_hash, z_i[1] + 1, z_i[2]]
/// ```
#[derive(Clone, Debug)]
pub struct AuctionStepCircuit {
    pub batch_size: usize,
}

impl FCircuit<Fr> for AuctionStepCircuit {
    type Params = usize;
    type ExternalInputs = AuctionExternalInputs;
    type ExternalInputsVar = AuctionExternalInputsVar;

    fn new(batch_size: Self::Params) -> Result<Self, Error> {
        Ok(Self { batch_size })
    }

    fn state_len(&self) -> usize {
        3 // [state_hash, round_nonce, policy_hash]
    }

    fn generate_step_constraints(
        &self,
        cs: ConstraintSystemRef<Fr>,
        _i: usize,
        z_i: Vec<FpVar<Fr>>,
        external_inputs: Self::ExternalInputsVar,
    ) -> Result<Vec<FpVar<Fr>>, SynthesisError> {
        let prev_state_hash = z_i[0].clone();
        let round_nonce = z_i[1].clone();
        let policy_hash = z_i[2].clone();

        let cfg = poseidon_config();
        let one = FpVar::<Fr>::one();
        let zero = FpVar::<Fr>::zero();
        let scale_factor = FpVar::<Fr>::constant(Fr::from(SCALE_FACTOR_I128 as u128));

        // Policy hash check: poseidon(min_size, min_price, position_limit) == z_i[2]
        {
            let mut ph_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            ph_sponge.absorb(
                &[
                    external_inputs.min_size.clone(),
                    external_inputs.min_price.clone(),
                    external_inputs.position_limit.clone(),
                ]
                .as_ref(),
            )?;
            let computed_policy_hash = ph_sponge.squeeze_field_elements(1)?[0].clone();
            computed_policy_hash.enforce_equal(&policy_hash)?;
        }

        let mut all_leaves: Vec<FpVar<Fr>> = Vec::with_capacity(self.batch_size * 2);
        let mut all_notionals: Vec<FpVar<Fr>> = Vec::with_capacity(self.batch_size);
        let mut active_count = FpVar::<Fr>::zero();

        for idx in 0..self.batch_size {
            // Either use a real match row or allocate a zero-filled inactive row
            // so the constraint count is constant regardless of how many real
            // matches external_inputs carries.
            let (
                bid_trader,
                bid_salt,
                bid_lp,
                bid_side,
                bid_balance,
                bid_position,
                bid_addr,
                bid_order_size,
                ask_trader,
                ask_salt,
                ask_lp,
                ask_side,
                ask_balance,
                ask_position,
                ask_addr,
                ask_order_size,
                m_price,
                m_size,
                is_active,
            ) = if idx < external_inputs.matches.len() {
                let cm = &external_inputs.matches[idx];
                (
                    cm.bid_trader.clone(),
                    cm.bid_salt.clone(),
                    cm.bid_limit_price.clone(),
                    cm.bid_side.clone(),
                    cm.bid_balance.clone(),
                    cm.bid_position.clone(),
                    cm.bid_trader_addr.clone(),
                    cm.bid_order_size.clone(),
                    cm.ask_trader.clone(),
                    cm.ask_salt.clone(),
                    cm.ask_limit_price.clone(),
                    cm.ask_side.clone(),
                    cm.ask_balance.clone(),
                    cm.ask_position.clone(),
                    cm.ask_trader_addr.clone(),
                    cm.ask_order_size.clone(),
                    cm.match_price.clone(),
                    cm.match_size.clone(),
                    cm.is_active.clone(),
                )
            } else {
                // Pad with an inactive row (ask_side=1 so Family 2 is satisfied
                // when is_active=0, which it is).
                let z = || Ok(Fr::zero());
                let o = || Ok(Fr::one());
                (
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), o)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                    FpVar::new_witness(cs.clone(), z)?,
                )
            };

            // ── Family 1: side bits ∈ {0,1}, is_active ∈ {0,1} ────────────
            (&bid_side * (&one - &bid_side)).enforce_equal(&zero)?;
            (&ask_side * (&one - &ask_side)).enforce_equal(&zero)?;
            (&is_active * (&one - &is_active)).enforce_equal(&zero)?;

            // ── Family 2: bid+ask sides are opposite on active rows ─────────
            ((&bid_side + &ask_side - &one) * &is_active).enforce_equal(&zero)?;

            // ── Family 4 (unconditional 60-bit bounds) ──────────────────────
            enforce_range_60(&m_size)?;
            enforce_range_60(&bid_lp)?;
            enforce_range_60(&ask_lp)?;
            enforce_range_60(&m_price)?;

            // ── Family 4 (cont'd): m_size >= min_size, bid_lp >= min_price ──
            let size_diff = (&m_size - &external_inputs.min_size) * &is_active;
            enforce_range_60(&size_diff)?;
            let bid_price_diff = (&bid_lp - &external_inputs.min_price) * &is_active;
            enforce_range_60(&bid_price_diff)?;

            // ── Family 3: price crossing bid_lp >= m_price >= ask_lp ────────
            let bid_cross = (&bid_lp - &m_price) * &is_active;
            enforce_range_60(&bid_cross)?;
            let ask_cross = (&m_price - &ask_lp) * &is_active;
            enforce_range_60(&ask_cross)?;

            // ── Family 3b: uniform clearing price ───────────────────────────
            // Every active row must settle at the single auction clearing
            // price. Without this the proof accepts a batch where the operator
            // gives different prices to different traders (#163).
            ((&m_price - &external_inputs.clearing_price) * &is_active).enforce_equal(&zero)?;

            // ── Family 5: commitment binding ─────────────────────────────────
            enforce_range_60(&bid_order_size)?;
            enforce_range_60(&ask_order_size)?;
            let bid_os_ge_ms = (&bid_order_size - &m_size) * &is_active;
            enforce_range_60(&bid_os_ge_ms)?;
            let ask_os_ge_ms = (&ask_order_size - &m_size) * &is_active;
            enforce_range_60(&ask_os_ge_ms)?;

            let mut bid_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            bid_sponge.absorb(
                &[
                    bid_trader.clone(),
                    bid_side.clone(),
                    bid_lp.clone(),
                    bid_order_size.clone(),
                    bid_salt.clone(),
                ]
                .as_ref(),
            )?;
            let bid_commit = bid_sponge.squeeze_field_elements(1)?[0].clone();

            let mut ask_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            ask_sponge.absorb(
                &[
                    ask_trader.clone(),
                    ask_side.clone(),
                    ask_lp.clone(),
                    ask_order_size.clone(),
                    ask_salt.clone(),
                ]
                .as_ref(),
            )?;
            let ask_commit = ask_sponge.squeeze_field_elements(1)?[0].clone();

            // ── Family 6: notional = price * size ────────────────────────────
            let notional = &m_price * &m_size;

            // ── Family 7: solvency ────────────────────────────────────────────
            enforce_range_60(&bid_balance)?;
            enforce_range_60(&ask_balance)?;
            let bid_solvency_diff = (&bid_balance * &scale_factor - &notional) * &is_active;
            enforce_range_n(&bid_solvency_diff, SOLVENCY_DIFF_BITS)?;
            let ask_solvency_diff = (&ask_balance * &scale_factor - &notional) * &is_active;
            enforce_range_n(&ask_solvency_diff, SOLVENCY_DIFF_BITS)?;

            // ── Family 8: position limit (two-sided) ──────────────────────────
            let bid_new_pos = &bid_position + &m_size;
            let ask_new_pos = &ask_position - &m_size;
            let bid_pos_lo = (&external_inputs.position_limit - &bid_new_pos) * &is_active;
            let bid_pos_hi = (&external_inputs.position_limit + &bid_new_pos) * &is_active;
            let ask_pos_lo = (&external_inputs.position_limit - &ask_new_pos) * &is_active;
            let ask_pos_hi = (&external_inputs.position_limit + &ask_new_pos) * &is_active;
            enforce_range_60(&bid_pos_lo)?;
            enforce_range_60(&bid_pos_hi)?;
            enforce_range_60(&ask_pos_lo)?;
            enforce_range_60(&ask_pos_hi)?;

            // ── Family 9: trader_id == poseidon(trader_addr) on active rows ──
            // Binds the in-circuit identity to the trader's on-chain settlement
            // address, so a proven match maps to the exact account the contract
            // debits/credits (#153). `trader_addr` is the address bytes as a
            // field element; the engine derives `trader_id` the same way from
            // the *verified* caller address, so a lying client cannot rebind.
            let mut bid_id_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            bid_id_sponge.absorb(&[bid_addr.clone()].as_ref())?;
            let bid_derived = bid_id_sponge.squeeze_field_elements(1)?[0].clone();
            ((&bid_derived - &bid_trader) * &is_active).enforce_equal(&zero)?;

            let mut ask_id_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            ask_id_sponge.absorb(&[ask_addr.clone()].as_ref())?;
            let ask_derived = ask_id_sponge.squeeze_field_elements(1)?[0].clone();
            ((&ask_derived - &ask_trader) * &is_active).enforce_equal(&zero)?;

            all_leaves.push(&bid_commit * &is_active);
            all_leaves.push(&ask_commit * &is_active);
            all_notionals.push(&notional * &is_active);
            active_count = &active_count + &is_active;
        }

        // ── Root binding ──────────────────────────────────────────────────────
        let mut root_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        root_sponge.absorb(&all_leaves.as_slice())?;
        let commitments_root_computed = root_sponge.squeeze_field_elements(1)?[0].clone();

        let mut nroot_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        nroot_sponge.absorb(&all_notionals.as_slice())?;
        let notionals_root_computed = nroot_sponge.squeeze_field_elements(1)?[0].clone();

        // ── State transition ──────────────────────────────────────────────────
        // new_state_hash = poseidon(prev_state_hash, commitments_root,
        //                           notionals_root, active_count)
        let mut st_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        st_sponge.absorb(
            &[
                prev_state_hash,
                commitments_root_computed,
                notionals_root_computed,
                active_count,
            ]
            .as_ref(),
        )?;
        let new_state_hash = st_sponge.squeeze_field_elements(1)?[0].clone();

        let new_round_nonce = &round_nonce + &one;

        Ok(vec![new_state_hash, new_round_nonce, policy_hash])
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn enforce_range_60(value: &FpVar<Fr>) -> Result<(), SynthesisError> {
    enforce_range_n(value, SIZE_BITS)
}

fn enforce_range_n(value: &FpVar<Fr>, n: usize) -> Result<(), SynthesisError> {
    let bits = value.to_bits_le()?;
    for b in bits.iter().skip(n) {
        b.enforce_equal(&Boolean::FALSE)?;
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pedersen::derive_trader_id;
    use crate::witness::{BatchWitness, MatchWitness, OrderLegWitness, DEFAULT_POLICY};
    use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
    use ark_crypto_primitives::sponge::CryptographicSponge;
    use ark_ff::BigInteger;
    use ark_ff::PrimeField;
    use ark_relations::gr1cs::ConstraintSystem;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    /// Trader id for a hex-encoded address, matching `trader_addr_scalar`'s
    /// projection: `poseidon(from_be_bytes_mod_order(addr_bytes))`.
    fn trader_id_hex(addr_hex: &str) -> String {
        let addr = hex::decode(addr_hex.trim_start_matches("0x")).unwrap();
        let f = derive_trader_id(&addr).unwrap();
        let mut bytes = f.into_bigint().to_bytes_be();
        while bytes.len() < 32 {
            bytes.insert(0, 0);
        }
        hex::encode(bytes)
    }

    fn sample_witness() -> (BatchWitness, Vec<Decimal>, Vec<Decimal>) {
        let bid_addr = "aa".repeat(20);
        let ask_addr = "bb".repeat(20);
        let m = MatchWitness {
            bid: OrderLegWitness {
                trader_id: trader_id_hex(&bid_addr),
                salt: "22".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(105),
                order_size: Decimal::from(10),
                side: 0,
                trader_addr: bid_addr,
            },
            ask: OrderLegWitness {
                trader_id: trader_id_hex(&ask_addr),
                salt: "44".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(95),
                order_size: Decimal::from(10),
                side: 1,
                trader_addr: ask_addr,
            },
        };
        let w = BatchWitness {
            batch_id: Uuid::nil(),
            auction_id: Uuid::nil(),
            matches: vec![m],
            policy: DEFAULT_POLICY.into_policy(),
        };
        (w, vec![Decimal::from(100)], vec![Decimal::from(10)])
    }

    fn run_step(
        circuit: &AuctionStepCircuit,
        z_i: Vec<Fr>,
        ext: AuctionExternalInputs,
    ) -> (bool, Vec<Fr>) {
        let cs = ConstraintSystem::<Fr>::new_ref();
        let z_i_vars: Vec<FpVar<Fr>> = z_i
            .iter()
            .map(|v| FpVar::new_witness(cs.clone(), || Ok(*v)).unwrap())
            .collect();
        let ext_var = AuctionExternalInputsVar::new_variable(
            cs.clone(),
            || Ok(&ext),
            AllocationMode::Witness,
        )
        .unwrap();
        let z_next = circuit
            .generate_step_constraints(cs.clone(), 0, z_i_vars, ext_var)
            .unwrap();
        let z_next_vals: Vec<Fr> = z_next.iter().map(|v| v.value().unwrap()).collect();
        (cs.is_satisfied().unwrap(), z_next_vals)
    }

    fn initial_z(ext: &AuctionExternalInputs) -> Vec<Fr> {
        let cfg = poseidon_config();
        let mut s = PoseidonSponge::<Fr>::new(&cfg);
        s.absorb(&vec![ext.min_size, ext.min_price, ext.position_limit]);
        let policy_hash = s.squeeze_field_elements::<Fr>(1)[0];
        vec![Fr::zero(), Fr::zero(), policy_hash]
    }

    #[test]
    fn step_satisfied_with_valid_witness() {
        let (w, prices, sizes) = sample_witness();
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(satisfied, "expected constraint system to be satisfied");
    }

    #[test]
    fn step_rejects_same_side() {
        let (mut w, prices, sizes) = sample_witness();
        w.matches[0].ask.side = 0; // both sides = bid
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "expected constraint system to be unsatisfied (same side)"
        );
    }

    #[test]
    fn step_rejects_match_price_above_bid_limit() {
        let (mut w, _, sizes) = sample_witness();
        // bid_limit = 105, match_price = 110 → crossing violated
        w.matches[0].bid.limit_price = Decimal::from(105);
        let ext =
            AuctionExternalInputs::from_witness(&w, &[Decimal::from(110)], &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "expected constraint system to be unsatisfied (price above bid limit)"
        );
    }

    #[test]
    fn step_rejects_insufficient_balance() {
        let (mut w, prices, sizes) = sample_witness();
        // notional = 100 * 10 = 1000; balance 500 < 1000 → solvency fails
        w.matches[0].bid.balance = Decimal::from(500);
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "expected constraint system to be unsatisfied (insufficient balance)"
        );
    }

    #[test]
    fn step_rejects_forged_trader_id() {
        let (mut w, prices, sizes) = sample_witness();
        // trader_id no longer matches poseidon(commitment_key)
        w.matches[0].bid.trader_id = "ab".repeat(32);
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "expected constraint system to be unsatisfied (forged trader id)"
        );
    }

    #[test]
    fn step_state_transition_updates_correctly() {
        let (w, prices, sizes) = sample_witness();
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, z_next) = run_step(&circuit, z_0.clone(), ext);
        assert!(satisfied, "expected valid step to satisfy constraints");
        // round_nonce incremented by 1
        assert_eq!(
            z_next[1],
            z_0[1] + Fr::one(),
            "round_nonce must increment by 1"
        );
        // policy_hash unchanged
        assert_eq!(
            z_next[2], z_0[2],
            "policy_hash must remain invariant across steps"
        );
        // state_hash changed
        assert_ne!(
            z_next[0], z_0[0],
            "state_hash must change after a non-trivial step"
        );
    }

    /// Two crossing matches at two different prices. Each crosses
    /// individually, so without the uniform-price constraint the batch is
    /// accepted — exactly the per-match price-discrimination gap (#163).
    fn two_match_witness(
        price0: Decimal,
        price1: Decimal,
    ) -> (BatchWitness, Vec<Decimal>, Vec<Decimal>) {
        let mk = |bid_addr: &str, ask_addr: &str| MatchWitness {
            bid: OrderLegWitness {
                trader_id: trader_id_hex(bid_addr),
                salt: "22".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(105),
                order_size: Decimal::from(10),
                side: 0,
                trader_addr: bid_addr.to_string(),
            },
            ask: OrderLegWitness {
                trader_id: trader_id_hex(ask_addr),
                salt: "44".repeat(32),
                balance: Decimal::from(1_000_000),
                position: "0".into(),
                limit_price: Decimal::from(95),
                order_size: Decimal::from(10),
                side: 1,
                trader_addr: ask_addr.to_string(),
            },
        };
        let w = BatchWitness {
            batch_id: Uuid::nil(),
            auction_id: Uuid::nil(),
            matches: vec![
                mk(&"a1".repeat(20), &"a2".repeat(20)),
                mk(&"b1".repeat(20), &"b2".repeat(20)),
            ],
            policy: DEFAULT_POLICY.into_policy(),
        };
        (
            w,
            vec![price0, price1],
            vec![Decimal::from(10), Decimal::from(10)],
        )
    }

    #[test]
    fn step_rejects_non_uniform_clearing_price() {
        let (w, prices, sizes) = two_match_witness(Decimal::from(100), Decimal::from(101));
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "expected unsatisfied: matches at different prices must be rejected"
        );
    }

    #[test]
    fn step_accepts_uniform_clearing_price() {
        let (w, prices, sizes) = two_match_witness(Decimal::from(100), Decimal::from(100));
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            satisfied,
            "expected satisfied: two active matches at the same price"
        );
    }
}
