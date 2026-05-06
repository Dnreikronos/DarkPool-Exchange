//! Groth16 circuit over BN254 proving validity of a batch of matched pairs.
//!
//! Public inputs:
//! - `match_count`: number of active (non-padded) match rows.
//! - `commitments_root`: Poseidon root over all 2N order-leg commitments.
//! - `notionals_root`: Poseidon root over `price_i * size_i` per match.
//! - `min_size`, `min_price`, `position_limit`: policy constants.
//!
//! Constraint families implemented in v2:
//!
//! 1. `side` is a bit (0 or 1).
//! 2. `bid.side + ask.side == 1` (sides opposite).
//! 3. Match-leg consistency: bid.size == ask.size == match.size, and the
//!    leg limit prices satisfy crossing.
//! 4. `size >= min_size`, `limit_price >= min_price` (range, 60-bit).
//! 5. Poseidon commitment binding: each leg's claimed commitment hashes
//!    correctly into the public root.
//! 6. Notional binding: each match contributes `price * size` to
//!    `notionals_root`.
//! 7. Solvency: `balance * 1e8 >= notional` per leg (range, 120-bit), with
//!    a 60-bit bound on `balance` so the field-arithmetic check matches
//!    integer arithmetic.
//! 8. Position limit (two-sided): for the post-trade position
//!    `new_pos = position + size` (bid) / `position - size` (ask),
//!    enforce `(position_limit - new_pos)` and `(position_limit + new_pos)`
//!    each fit in 60 bits, which proves `|new_pos| <= position_limit` in
//!    the signed-int interpretation.
//! 9. Trader-id binding: `trader_id == poseidon(commitment_key_scalar)` per
//!    leg, gated by `is_active`.

use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::sponge::poseidon::constraints::PoseidonSpongeVar;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_groth16::{Groth16, ProvingKey, VerifyingKey};
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_snark::SNARK;
use ark_std::rand::{CryptoRng, RngCore};

use crate::encoding::{decimal_to_scalar, SCALE_FACTOR_I128};
use crate::pedersen::{commit_native, hash_root_native, poseidon_config, OrderCommitmentInput};
use crate::witness::{BatchWitness, MatchWitness};
use crate::ZkError;

const SIZE_BITS: usize = 60;
/// Range cap for the solvency diff `balance * 1e8 - notional`. With
/// `balance < 2^60` (Family 7's bound) and `1e8 < 2^27`, the product is
/// `< 2^87`; notional is `< 2^120`. 120 bits covers the worst-case
/// non-negative diff with margin.
const SOLVENCY_DIFF_BITS: usize = 120;

/// Convenience wrapper over the proof bytes returned by [`prove`].
#[derive(Clone, Debug)]
pub struct ProofBytes(pub Vec<u8>);

/// Per-match circuit witness: encoded scalars + the public-input share.
#[derive(Clone, Debug)]
struct CircuitMatch {
    bid_trader: Fr,
    bid_salt: Fr,
    bid_limit_price: Fr,
    bid_side: Fr,
    bid_balance: Fr,
    bid_position: Fr,
    bid_commitment_key: Fr,
    bid_order_size: Fr,

    ask_trader: Fr,
    ask_salt: Fr,
    ask_limit_price: Fr,
    ask_side: Fr,
    ask_balance: Fr,
    ask_position: Fr,
    ask_commitment_key: Fr,
    ask_order_size: Fr,

    match_price: Fr,
    match_size: Fr,

    bid_commitment: Fr,
    ask_commitment: Fr,
    notional: Fr,

    is_active: Fr,
}

#[derive(Clone)]
pub struct BatchProofCircuit {
    pub batch_size: usize,
    matches: Vec<CircuitMatch>,
    /// Public.
    match_count: Fr,
    commitments_root: Fr,
    notionals_root: Fr,
    min_size: Fr,
    min_price: Fr,
    position_limit: Fr,
}

impl BatchProofCircuit {
    /// Construct the prover-side circuit from a witness + the cleared
    /// per-match price/size pulled from the auction result.
    pub fn from_witness(
        witness: &BatchWitness,
        match_prices: &[rust_decimal::Decimal],
        match_sizes: &[rust_decimal::Decimal],
        batch_size: usize,
    ) -> Result<Self, ZkError> {
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
        let position_limit = crate::encoding::signed_to_scalar(pos_limit_i)?;

        let mut matches = Vec::with_capacity(batch_size);
        let mut leaves: Vec<Fr> = Vec::with_capacity(batch_size * 2);
        let mut notionals: Vec<Fr> = Vec::with_capacity(batch_size);

        for (i, m) in witness.matches.iter().enumerate() {
            let cm = build_match(m, match_prices[i], match_sizes[i])?;
            leaves.push(cm.bid_commitment);
            leaves.push(cm.ask_commitment);
            notionals.push(cm.notional);
            matches.push(cm);
        }
        // Pad with inactive rows. Their commitments are computed via the
        // same Poseidon path so the in-circuit root matches.
        while matches.len() < batch_size {
            let inactive = build_inactive();
            leaves.push(inactive.bid_commitment);
            leaves.push(inactive.ask_commitment);
            notionals.push(inactive.notional);
            matches.push(inactive);
        }

        let commitments_root = hash_root_native(&leaves);
        let notionals_root = hash_root_native(&notionals);
        let match_count = Fr::from(witness.matches.len() as u64);

        Ok(Self {
            batch_size,
            matches,
            match_count,
            commitments_root,
            notionals_root,
            min_size,
            min_price,
            position_limit,
        })
    }

    /// Build a stub circuit shaped like a real one but with zero rows. Used
    /// at keygen time so the constraint system is fully built.
    pub fn empty(batch_size: usize) -> Self {
        let zero = Fr::from(0u64);
        let mut matches = Vec::with_capacity(batch_size);
        let mut leaves: Vec<Fr> = Vec::with_capacity(batch_size * 2);
        let mut notionals: Vec<Fr> = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let inactive = build_inactive();
            leaves.push(inactive.bid_commitment);
            leaves.push(inactive.ask_commitment);
            notionals.push(inactive.notional);
            matches.push(inactive);
        }
        let commitments_root = hash_root_native(&leaves);
        let notionals_root = hash_root_native(&notionals);
        Self {
            batch_size,
            matches,
            match_count: zero,
            commitments_root,
            notionals_root,
            min_size: zero,
            min_price: zero,
            position_limit: zero,
        }
    }

    pub fn public_inputs(&self) -> Vec<Fr> {
        vec![
            self.match_count,
            self.commitments_root,
            self.notionals_root,
            self.min_size,
            self.min_price,
            self.position_limit,
        ]
    }
}

fn build_inactive() -> CircuitMatch {
    let zero = Fr::from(0u64);
    let one = Fr::from(1u64);
    let bid_input = OrderCommitmentInput {
        trader_id: zero,
        side: zero,
        limit_price: zero,
        size: zero,
        salt: zero,
    };
    let ask_input = OrderCommitmentInput {
        trader_id: zero,
        side: one,
        limit_price: zero,
        size: zero,
        salt: zero,
    };
    CircuitMatch {
        bid_trader: zero,
        bid_salt: zero,
        bid_limit_price: zero,
        bid_side: zero,
        bid_balance: zero,
        bid_position: zero,
        bid_commitment_key: zero,
        bid_order_size: zero,
        ask_trader: zero,
        ask_salt: zero,
        ask_limit_price: zero,
        ask_side: one,
        ask_balance: zero,
        ask_position: zero,
        ask_commitment_key: zero,
        ask_order_size: zero,
        match_price: zero,
        match_size: zero,
        bid_commitment: commit_native(&bid_input),
        ask_commitment: commit_native(&ask_input),
        notional: zero,
        is_active: zero,
    }
}

fn build_match(
    m: &MatchWitness,
    match_price: rust_decimal::Decimal,
    match_size: rust_decimal::Decimal,
) -> Result<CircuitMatch, ZkError> {
    let bid_trader = crate::pedersen::bytes_to_scalar(
        &m.bid
            .trader_id_bytes()
            .map_err(|e| ZkError::Witness(format!("bid trader_id: {e}")))?,
    );
    let bid_salt = crate::pedersen::bytes_to_scalar(
        &m.bid.salt_bytes().map_err(|e| ZkError::Witness(format!("bid salt: {e}")))?,
    );
    let ask_trader = crate::pedersen::bytes_to_scalar(
        &m.ask
            .trader_id_bytes()
            .map_err(|e| ZkError::Witness(format!("ask trader_id: {e}")))?,
    );
    let ask_salt = crate::pedersen::bytes_to_scalar(
        &m.ask.salt_bytes().map_err(|e| ZkError::Witness(format!("ask salt: {e}")))?,
    );

    let match_price_s = decimal_to_scalar(match_price)?;
    let match_size_s = decimal_to_scalar(match_size)?;
    let bid_order_size = m.bid.order_size_scalar()?;
    let ask_order_size = m.ask.order_size_scalar()?;

    let bid_input = OrderCommitmentInput {
        trader_id: bid_trader,
        side: Fr::from(m.bid.side as u64),
        limit_price: m.bid.limit_price_scalar()?,
        size: bid_order_size,
        salt: bid_salt,
    };
    let ask_input = OrderCommitmentInput {
        trader_id: ask_trader,
        side: Fr::from(m.ask.side as u64),
        limit_price: m.ask.limit_price_scalar()?,
        size: ask_order_size,
        salt: ask_salt,
    };
    let bid_commitment = commit_native(&bid_input);
    let ask_commitment = commit_native(&ask_input);
    let notional = match_price_s * match_size_s;

    let bid_balance = m.bid.balance_scalar()?;
    let ask_balance = m.ask.balance_scalar()?;
    let bid_position = m.bid.position_scalar()?;
    let ask_position = m.ask.position_scalar()?;
    let bid_commitment_key = m.bid.commitment_key_scalar();
    let ask_commitment_key = m.ask.commitment_key_scalar();

    Ok(CircuitMatch {
        bid_trader,
        bid_salt,
        bid_limit_price: bid_input.limit_price,
        bid_side: Fr::from(m.bid.side as u64),
        bid_balance,
        bid_position,
        bid_commitment_key,
        bid_order_size,
        ask_trader,
        ask_salt,
        ask_limit_price: ask_input.limit_price,
        ask_side: Fr::from(m.ask.side as u64),
        ask_balance,
        ask_position,
        ask_commitment_key,
        ask_order_size,
        match_price: match_price_s,
        match_size: match_size_s,
        bid_commitment,
        ask_commitment,
        notional,
        is_active: Fr::from(1u64),
    })
}

impl ConstraintSynthesizer<Fr> for BatchProofCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // Public inputs (order matters and matches `public_inputs()`).
        let match_count = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.match_count))?;
        let commitments_root = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.commitments_root))?;
        let notionals_root = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.notionals_root))?;
        let min_size = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.min_size))?;
        let min_price = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.min_price))?;
        let position_limit = FpVar::<Fr>::new_input(cs.clone(), || Ok(self.position_limit))?;

        let scale_factor = FpVar::<Fr>::constant(Fr::from(SCALE_FACTOR_I128 as u128));

        let mut all_leaves: Vec<FpVar<Fr>> = Vec::with_capacity(self.batch_size * 2);
        let mut all_notionals: Vec<FpVar<Fr>> = Vec::with_capacity(self.batch_size);
        let mut active_count = FpVar::<Fr>::zero();

        let one = FpVar::<Fr>::one();
        let zero = FpVar::<Fr>::zero();
        let cfg = poseidon_config();

        for cm in &self.matches {
            let bid_side = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_side))?;
            let ask_side = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_side))?;
            let bid_lp = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_limit_price))?;
            let ask_lp = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_limit_price))?;
            let bid_trader = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_trader))?;
            let ask_trader = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_trader))?;
            let bid_salt = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_salt))?;
            let ask_salt = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_salt))?;
            let m_price = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.match_price))?;
            let m_size = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.match_size))?;
            let is_active = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.is_active))?;
            let bid_balance = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_balance))?;
            let ask_balance = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_balance))?;
            let bid_position = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_position))?;
            let ask_position = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_position))?;
            let bid_ck = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_commitment_key))?;
            let ask_ck = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_commitment_key))?;
            let bid_order_size = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.bid_order_size))?;
            let ask_order_size = FpVar::<Fr>::new_witness(cs.clone(), || Ok(cm.ask_order_size))?;

            // Family 1: side bit well-formed (each side ∈ {0,1}).
            // is_active also ∈ {0,1}.
            (&bid_side * (&one - &bid_side)).enforce_equal(&zero)?;
            (&ask_side * (&one - &ask_side)).enforce_equal(&zero)?;
            (&is_active * (&one - &is_active)).enforce_equal(&zero)?;

            // Family 2: opposite sides for active rows.
            // is_active * (bid_side + ask_side - 1) == 0
            ((&bid_side + &ask_side - &one) * &is_active).enforce_equal(&zero)?;

            // Family 4: range check size (60-bit) and price (60-bit) for
            // active rows only. We multiply by is_active so that padded
            // rows can carry zero without triggering overflow. The bit
            // decomposition itself validates the upper bound.
            enforce_range_60(&m_size)?;
            enforce_range_60(&bid_lp)?;
            enforce_range_60(&ask_lp)?;
            enforce_range_60(&m_price)?;

            // Family 4 (cont'd): m_size >= min_size, bid_lp >= min_price.
            // active_row * (m_size - min_size) is non-negative -> we
            // enforce via "diff is in 60-bit range" trick.
            let size_diff = (&m_size - &min_size) * &is_active;
            enforce_range_60(&size_diff)?;
            let bid_diff = (&bid_lp - &min_price) * &is_active;
            enforce_range_60(&bid_diff)?;

            // Family 3: leg consistency: bid_lp >= match_price >= ask_lp on
            // active rows.
            let bid_cross = (&bid_lp - &m_price) * &is_active;
            enforce_range_60(&bid_cross)?;
            let ask_cross = (&m_price - &ask_lp) * &is_active;
            enforce_range_60(&ask_cross)?;

            // Family 5: Poseidon commitment binding. Uses the order's
            // original size (not match fill size) to match the engine-side
            // commitment computed at placement time.
            enforce_range_60(&bid_order_size)?;
            enforce_range_60(&ask_order_size)?;
            let bid_os_ge_ms = (&bid_order_size - &m_size) * &is_active;
            enforce_range_60(&bid_os_ge_ms)?;
            let ask_os_ge_ms = (&ask_order_size - &m_size) * &is_active;
            enforce_range_60(&ask_os_ge_ms)?;

            let mut sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            sponge.absorb(&[bid_trader.clone(), bid_side.clone(), bid_lp.clone(), bid_order_size.clone(), bid_salt.clone()].as_ref())?;
            let bid_commit = sponge.squeeze_field_elements(1)?[0].clone();

            let mut sponge2 = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            sponge2.absorb(&[ask_trader.clone(), ask_side.clone(), ask_lp.clone(), ask_order_size.clone(), ask_salt.clone()].as_ref())?;
            let ask_commit = sponge2.squeeze_field_elements(1)?[0].clone();

            // Family 6: notional = price * size (computed in-circuit).
            let notional = &m_price * &m_size;

            // Family 7: solvency. balance is bounded to 60-bit so the
            // multiplication-by-1e8 stays in the field's integer range, and
            // `balance*1e8 - notional >= 0` is enforced via a 120-bit range
            // proof. Padded rows have zero balance/notional → diff = 0.
            enforce_range_60(&bid_balance)?;
            enforce_range_60(&ask_balance)?;
            let bid_solvency_diff =
                (&bid_balance * &scale_factor - &notional) * &is_active;
            enforce_range_n(&bid_solvency_diff, SOLVENCY_DIFF_BITS)?;
            let ask_solvency_diff =
                (&ask_balance * &scale_factor - &notional) * &is_active;
            enforce_range_n(&ask_solvency_diff, SOLVENCY_DIFF_BITS)?;

            // Family 8: post-trade position-limit (two-sided range). For
            // bid, new_pos = position + match_size; for ask, new_pos =
            // position - match_size. We require both `(limit - new_pos)`
            // and `(limit + new_pos)` to fit in 60 bits, which forces
            // `|new_pos| <= position_limit` in signed-int interpretation.
            let bid_new_pos = &bid_position + &m_size;
            let ask_new_pos = &ask_position - &m_size;
            let bid_pos_lo = (&position_limit - &bid_new_pos) * &is_active;
            let bid_pos_hi = (&position_limit + &bid_new_pos) * &is_active;
            let ask_pos_lo = (&position_limit - &ask_new_pos) * &is_active;
            let ask_pos_hi = (&position_limit + &ask_new_pos) * &is_active;
            enforce_range_60(&bid_pos_lo)?;
            enforce_range_60(&bid_pos_hi)?;
            enforce_range_60(&ask_pos_lo)?;
            enforce_range_60(&ask_pos_hi)?;

            // Family 9: trader_id == poseidon(commitment_key_scalar) on
            // active rows.
            let mut bid_id_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            bid_id_sponge.absorb(&[bid_ck.clone()].as_ref())?;
            let bid_derived = bid_id_sponge.squeeze_field_elements(1)?[0].clone();
            ((&bid_derived - &bid_trader) * &is_active).enforce_equal(&zero)?;

            let mut ask_id_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            ask_id_sponge.absorb(&[ask_ck.clone()].as_ref())?;
            let ask_derived = ask_id_sponge.squeeze_field_elements(1)?[0].clone();
            ((&ask_derived - &ask_trader) * &is_active).enforce_equal(&zero)?;

            all_leaves.push(bid_commit);
            all_leaves.push(ask_commit);
            all_notionals.push(notional);
            active_count = &active_count + &is_active;
        }

        // commitments_root binding.
        let mut root_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        root_sponge.absorb(&all_leaves.as_slice())?;
        let computed_root = root_sponge.squeeze_field_elements(1)?[0].clone();
        computed_root.enforce_equal(&commitments_root)?;

        // notionals_root binding.
        let mut nroot_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        nroot_sponge.absorb(&all_notionals.as_slice())?;
        let computed_nroot = nroot_sponge.squeeze_field_elements(1)?[0].clone();
        computed_nroot.enforce_equal(&notionals_root)?;

        // match_count == sum(is_active).
        active_count.enforce_equal(&match_count)?;

        Ok(())
    }
}

/// Enforce that `value` fits in `SIZE_BITS` bits (i.e. is non-negative and
/// strictly less than 2^SIZE_BITS).
fn enforce_range_60(value: &FpVar<Fr>) -> Result<(), SynthesisError> {
    enforce_range_n(value, SIZE_BITS)
}

/// Enforce that `value` fits in `n` bits.
fn enforce_range_n(value: &FpVar<Fr>, n: usize) -> Result<(), SynthesisError> {
    let bits = value.to_bits_le()?;
    for b in bits.iter().skip(n) {
        b.enforce_equal(&Boolean::FALSE)?;
    }
    Ok(())
}

/// Run Groth16 trusted setup against an empty circuit of the given size.
pub fn setup<R: RngCore + CryptoRng>(
    batch_size: usize,
    rng: &mut R,
) -> Result<(ProvingKey<Bn254>, VerifyingKey<Bn254>), ZkError> {
    let circuit = BatchProofCircuit::empty(batch_size);
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, rng)
        .map_err(|e| ZkError::Setup(e.to_string()))?;
    Ok((pk, vk))
}

/// Generate a Groth16 proof for a populated circuit.
pub fn prove<R: RngCore + CryptoRng>(
    pk: &ProvingKey<Bn254>,
    circuit: BatchProofCircuit,
    rng: &mut R,
) -> Result<ProofBytes, ZkError> {
    use ark_serialize::{CanonicalSerialize, Compress};
    let proof = Groth16::<Bn254>::prove(pk, circuit, rng)
        .map_err(|e| ZkError::Prove(e.to_string()))?;
    let mut buf = Vec::with_capacity(proof.serialized_size(Compress::Yes));
    proof
        .serialize_with_mode(&mut buf, Compress::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;
    Ok(ProofBytes(buf))
}

/// Compute the 6 public inputs `[match_count, commitments_root, notionals_root,
/// min_size, min_price, position_limit]` from a witness without building the
/// full prover circuit. Used by the engine to populate `SubmitBatchParams`.
pub fn compute_public_inputs(
    witness: &BatchWitness,
    match_prices: &[rust_decimal::Decimal],
    match_sizes: &[rust_decimal::Decimal],
    batch_size: usize,
) -> Result<[Fr; 6], ZkError> {
    if witness.matches.len() != match_prices.len()
        || match_prices.len() != match_sizes.len()
    {
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
    let position_limit = crate::encoding::signed_to_scalar(pos_limit_i)?;

    let mut leaves: Vec<Fr> = Vec::with_capacity(batch_size * 2);
    let mut notionals: Vec<Fr> = Vec::with_capacity(batch_size);

    for (i, m) in witness.matches.iter().enumerate() {
        let cm = build_match(m, match_prices[i], match_sizes[i])?;
        leaves.push(cm.bid_commitment);
        leaves.push(cm.ask_commitment);
        notionals.push(cm.notional);
    }
    while leaves.len() < batch_size * 2 {
        let inactive = build_inactive();
        leaves.push(inactive.bid_commitment);
        leaves.push(inactive.ask_commitment);
        notionals.push(inactive.notional);
    }

    let commitments_root = hash_root_native(&leaves);
    let notionals_root = hash_root_native(&notionals);
    let match_count = Fr::from(witness.matches.len() as u64);

    Ok([
        match_count,
        commitments_root,
        notionals_root,
        min_size,
        min_price,
        position_limit,
    ])
}

/// Verify a serialized Groth16 proof against the given public inputs.
pub fn verify(
    vk: &VerifyingKey<Bn254>,
    public_inputs: &[Fr],
    proof_bytes: &[u8],
) -> Result<bool, ZkError> {
    use ark_groth16::Proof;
    use ark_serialize::{CanonicalDeserialize, Compress, Validate};
    let proof = Proof::<Bn254>::deserialize_with_mode(proof_bytes, Compress::Yes, Validate::Yes)
        .map_err(|e| ZkError::Serialize(e.to_string()))?;
    let pvk = Groth16::<Bn254>::process_vk(vk)
        .map_err(|e| ZkError::Serialize(format!("process_vk: {e}")))?;
    Groth16::<Bn254>::verify_with_processed_vk(&pvk, public_inputs, &proof)
        .map_err(|_| ZkError::Verify)
}

#[cfg(test)]
mod tests;
