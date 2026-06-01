use std::borrow::Borrow;

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::constraints::PoseidonSpongeVar;
use ark_crypto_primitives::sponge::poseidon::PoseidonConfig;
use ark_ff::{One, Zero};
use ark_r1cs_std::alloc::{AllocVar, AllocationMode};
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::ToBitsGadget;
use ark_r1cs_std::prelude::*;
use ark_relations::gr1cs::{ConstraintSystemRef, Namespace, SynthesisError};
use folding_schemes::{frontend::FCircuit, Error};

use crate::encoding::SCALE_FACTOR_I128;
use crate::merkle::{admitted_set_proof, admitted_set_root, MerkleProof, MERKLE_DEPTH};
use crate::pedersen::{bytes_to_scalar, commit_native, poseidon_config, OrderCommitmentInput};
use crate::witness::BatchWitness;
use crate::ZkError;

const SIZE_BITS: usize = 60;
const SOLVENCY_DIFF_BITS: usize = 120;

/// Fixed number of match rows the IVC step circuit processes every round.
///
/// HyperNova derives the constraint system once, during preprocessing, by
/// running the step circuit on [`AuctionExternalInputs::default`]. That CCS
/// must be byte-for-byte the same shape as the one used while proving, so the
/// circuit width cannot depend on how many real matches a given round carries.
/// Every round is therefore padded to exactly `IVC_BATCH_SIZE` rows (inactive
/// rows pass all constraints and contribute nothing to the state). A round's
/// `batch_size` is a per-round capacity hint and must not exceed this width.
pub const IVC_BATCH_SIZE: usize = 8;

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

    /// Membership paths proving the `bid` / `ask` commitments are leaves of the
    /// round's admitted-set Merkle root (#157). Default (all-zero) on inactive
    /// padding rows — the in-circuit membership check is gated by `is_active`.
    pub bid_path: MerkleProof,
    pub ask_path: MerkleProof,
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
            bid_path: MerkleProof::default(),
            ask_path: MerkleProof::default(),
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
    /// match in [`Self::from_witness`]; the honest engine applies one volume-
    /// maximizing price to every match, so all rows already equal it.
    pub clearing_price: Fr,
    /// Canonical Merkle root over the commitments of every order admitted to
    /// this auction round (#157). Folded into `z[4]` so a watcher can recompute
    /// it from the public `OrderPlaced` log and confirm the operator did not
    /// match an order outside the publicly admitted set.
    pub admitted_root: Fr,
    pub matches: Vec<CircuitMatchNative>,
}

impl Default for AuctionExternalInputs {
    fn default() -> Self {
        Self {
            min_size: Fr::zero(),
            min_price: Fr::zero(),
            position_limit: Fr::zero(),
            clearing_price: Fr::zero(),
            admitted_root: Fr::zero(),
            matches: Vec::new(),
        }
    }
}

impl AuctionExternalInputs {
    /// Build the policy + match rows from a [`BatchWitness`], padding inactive
    /// rows to `batch_size`. Leaves `admitted_root` and every per-row
    /// membership path at their defaults; callers finish the value via
    /// [`Self::attach_membership`].
    fn build_rows(
        witness: &BatchWitness,
        match_prices: &[rust_decimal::Decimal],
        match_sizes: &[rust_decimal::Decimal],
        batch_size: usize,
    ) -> Result<Self, ZkError> {
        use crate::encoding::{decimal_to_scalar, signed_to_scalar};

        if batch_size > IVC_BATCH_SIZE {
            return Err(ZkError::Witness(format!(
                "batch_size {batch_size} exceeds circuit width IVC_BATCH_SIZE {IVC_BATCH_SIZE}"
            )));
        }
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
                bid_path: MerkleProof::default(),
                ask_path: MerkleProof::default(),
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
            admitted_root: Fr::zero(),
            matches,
        })
    }

    /// Convert a [`BatchWitness`] + per-match prices/sizes into
    /// [`AuctionExternalInputs`]. The admitted set defaults to *exactly the
    /// matched legs*, so every settled order is trivially a member — useful in
    /// tests and as a safe fallback. The engine instead calls
    /// [`Self::from_witness_with_admitted`] with the full live order book (a
    /// superset), which is what turns membership into a real
    /// input-completeness constraint (#157).
    pub fn from_witness(
        witness: &BatchWitness,
        match_prices: &[rust_decimal::Decimal],
        match_sizes: &[rust_decimal::Decimal],
        batch_size: usize,
    ) -> Result<Self, ZkError> {
        let mut ext = Self::build_rows(witness, match_prices, match_sizes, batch_size)?;
        let admitted: Vec<Fr> = ext
            .matches
            .iter()
            .filter(|m| m.is_active == Fr::one())
            .flat_map(|m| [leaf_of(m, true), leaf_of(m, false)])
            .collect();
        ext.attach_membership(&admitted)?;
        Ok(ext)
    }

    /// Like [`Self::from_witness`] but binds the round to an explicit admitted
    /// set — the commitments of *every* order live in the book at tick time.
    /// Membership then proves each settled leg was drawn from the publicly
    /// admitted set, not injected off-log (#157). Errors if a matched leg's
    /// commitment is absent from `admitted_commitments` (an operator bug — a
    /// settled order that was never admitted) or if the set exceeds the tree
    /// capacity.
    pub fn from_witness_with_admitted(
        witness: &BatchWitness,
        match_prices: &[rust_decimal::Decimal],
        match_sizes: &[rust_decimal::Decimal],
        batch_size: usize,
        admitted_commitments: &[Fr],
    ) -> Result<Self, ZkError> {
        let mut ext = Self::build_rows(witness, match_prices, match_sizes, batch_size)?;
        ext.attach_membership(admitted_commitments)?;
        Ok(ext)
    }

    /// Compute the admitted-set root and fill in each active row's membership
    /// path against `admitted_commitments`.
    fn attach_membership(&mut self, admitted_commitments: &[Fr]) -> Result<(), ZkError> {
        let root = admitted_set_root(admitted_commitments)?;
        for m in self.matches.iter_mut() {
            if m.is_active != Fr::one() {
                // Padding rows keep default (all-zero) paths; the in-circuit
                // membership check is gated off by `is_active`.
                continue;
            }
            let bid_leaf = leaf_of(m, true);
            let ask_leaf = leaf_of(m, false);
            let (_, bid_path) =
                admitted_set_proof(admitted_commitments, bid_leaf)?.ok_or_else(|| {
                    ZkError::Witness("matched bid leg absent from admitted set".into())
                })?;
            let (_, ask_path) =
                admitted_set_proof(admitted_commitments, ask_leaf)?.ok_or_else(|| {
                    ZkError::Witness("matched ask leg absent from admitted set".into())
                })?;
            m.bid_path = bid_path;
            m.ask_path = ask_path;
        }
        self.admitted_root = root;
        Ok(())
    }
}

/// The Poseidon commitment of one leg of a match, computed from the row's
/// scalar fields. Identical to the in-circuit `bid_commit` / `ask_commit`
/// (and to the engine's persisted `OrderPlaced` commitment), so a matched
/// leg's leaf lines up with its entry in the admitted set.
fn leaf_of(m: &CircuitMatchNative, is_bid: bool) -> Fr {
    let input = if is_bid {
        OrderCommitmentInput {
            trader_id: m.bid_trader,
            side: m.bid_side,
            limit_price: m.bid_limit_price,
            size: m.bid_order_size,
            salt: m.bid_salt,
        }
    } else {
        OrderCommitmentInput {
            trader_id: m.ask_trader,
            side: m.ask_side,
            limit_price: m.ask_limit_price,
            size: m.ask_order_size,
            salt: m.ask_salt,
        }
    };
    commit_native(&input)
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
    pub bid_path: MerklePathVar,
    pub ask_path: MerklePathVar,
}

/// In-circuit version of [`MerkleProof`]: a fixed `MERKLE_DEPTH` siblings and
/// position bits. Allocated at constant width so the constraint system shape
/// never depends on the admitted set's size.
#[derive(Clone, Debug)]
pub struct MerklePathVar {
    pub siblings: Vec<FpVar<Fr>>,
    pub index_bits: Vec<Boolean<Fr>>,
}

/// In-circuit version of [`AuctionExternalInputs`].
#[derive(Clone, Debug)]
pub struct AuctionExternalInputsVar {
    pub min_size: FpVar<Fr>,
    pub min_price: FpVar<Fr>,
    pub position_limit: FpVar<Fr>,
    pub clearing_price: FpVar<Fr>,
    pub admitted_root: FpVar<Fr>,
    pub matches: Vec<CircuitMatchVar>,
}

/// Allocate a fixed-width membership path. Always exactly `MERKLE_DEPTH` rows
/// so preprocessing (run on the empty default) and proving derive the same CCS.
fn alloc_merkle_path(
    cs: &ConstraintSystemRef<Fr>,
    proof: &MerkleProof,
    mode: AllocationMode,
) -> Result<MerklePathVar, SynthesisError> {
    let mut siblings = Vec::with_capacity(MERKLE_DEPTH);
    let mut index_bits = Vec::with_capacity(MERKLE_DEPTH);
    for d in 0..MERKLE_DEPTH {
        siblings.push(FpVar::new_variable(
            cs.clone(),
            || Ok(proof.siblings[d]),
            mode,
        )?);
        index_bits.push(Boolean::new_variable(
            cs.clone(),
            || Ok(proof.index_bits[d]),
            mode,
        )?);
    }
    Ok(MerklePathVar {
        siblings,
        index_bits,
    })
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
        let admitted_root = FpVar::new_variable(cs.clone(), || Ok(native.admitted_root), mode)?;

        // Always allocate a fixed `IVC_BATCH_SIZE` rows so the constraint system
        // is identical whether `native` is the empty `default()` (used by
        // HyperNova preprocessing to derive the CCS) or a real, padded witness.
        // Short inputs are backfilled with inactive rows; over-long inputs would
        // be silently truncated by the loop below, so reject them outright — a
        // dropped match would settle on-chain without ever being proven.
        assert!(
            native.matches.len() <= IVC_BATCH_SIZE,
            "AuctionExternalInputs carries {} matches, exceeds circuit width IVC_BATCH_SIZE {}",
            native.matches.len(),
            IVC_BATCH_SIZE
        );
        let mut matches = Vec::with_capacity(IVC_BATCH_SIZE);
        for i in 0..IVC_BATCH_SIZE {
            let cm = native.matches.get(i).cloned().unwrap_or_default();
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
                bid_path: alloc_merkle_path(&cs, &cm.bid_path, mode)?,
                ask_path: alloc_merkle_path(&cs, &cm.ask_path, mode)?,
            });
        }

        Ok(Self {
            min_size,
            min_price,
            position_limit,
            clearing_price,
            admitted_root,
            matches,
        })
    }
}

// ─── Step circuit ─────────────────────────────────────────────────────────────

/// IVC step circuit for HyperNova.
///
/// IVC state:
/// `z_i = [state_hash, round_nonce, policy_hash, settlement_acc, admit_chain]`
/// (5 Fr elements).
///
/// Each step folds one auction batch, enforcing the constraint families, then
/// updates the state:
/// ```text
/// policy_hash_computed = poseidon(min_size, min_price, position_limit)
/// // checked == z_i[2]
/// new_state_hash = poseidon(z_i[0], commitments_root, notionals_root, active_count)
/// // settlement_acc folds each active row's tuple (bid_addr, ask_addr,
/// // match_price, match_size) into a running Poseidon chain (#153)
/// // each active leg's commitment is proven a member of the round's
/// // admitted-set Merkle root (#157); admit_chain folds that root in:
/// //   admit_chain' = poseidon(z_i[4], admitted_root)
/// z_{i+1} = [new_state_hash, z_i[1] + 1, z_i[2], settlement_acc', admit_chain']
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
        5 // [state_hash, round_nonce, policy_hash, settlement_acc, admit_chain]
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
        // Running Poseidon hash-chain over the plaintext settlement tuples of
        // every active match across the whole session, in order. The contract
        // recomputes the identical chain over the settled `matches[]` and
        // requires it equal `z_n[3]` — this is what binds on-chain settlement
        // to the proof (#153). A chain (not a sponge over a padded vector)
        // means the contract folds only real matches, with no padding-length
        // coupling to the circuit's fixed `batch_size`.
        let mut settlement_acc = z_i[3].clone();
        // Running chain over each round's admitted-set Merkle root (#157). The
        // root is also the value each active leg's membership path must verify
        // to, so folding it here binds the proof to the exact input set a
        // watcher recomputes from the public OrderPlaced log.
        let admit_chain_prev = z_i[4].clone();

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

        assert!(
            self.batch_size <= IVC_BATCH_SIZE,
            "batch_size {} exceeds IVC_BATCH_SIZE {}",
            self.batch_size,
            IVC_BATCH_SIZE
        );

        let mut all_leaves: Vec<FpVar<Fr>> = Vec::with_capacity(IVC_BATCH_SIZE * 2);
        let mut all_notionals: Vec<FpVar<Fr>> = Vec::with_capacity(IVC_BATCH_SIZE);
        let mut active_count = FpVar::<Fr>::zero();

        // `external_inputs.matches` always holds exactly `IVC_BATCH_SIZE` rows
        // (allocated by `AuctionExternalInputsVar`), so every row is a real
        // variable. Inactive padding rows carry `is_active = 0` and satisfy each
        // gated constraint trivially.
        for idx in 0..IVC_BATCH_SIZE {
            let cm = &external_inputs.matches[idx];
            let bid_trader = cm.bid_trader.clone();
            let bid_salt = cm.bid_salt.clone();
            let bid_lp = cm.bid_limit_price.clone();
            let bid_side = cm.bid_side.clone();
            let bid_balance = cm.bid_balance.clone();
            let bid_position = cm.bid_position.clone();
            let bid_addr = cm.bid_trader_addr.clone();
            let bid_order_size = cm.bid_order_size.clone();
            let ask_trader = cm.ask_trader.clone();
            let ask_salt = cm.ask_salt.clone();
            let ask_lp = cm.ask_limit_price.clone();
            let ask_side = cm.ask_side.clone();
            let ask_balance = cm.ask_balance.clone();
            let ask_position = cm.ask_position.clone();
            let ask_addr = cm.ask_trader_addr.clone();
            let ask_order_size = cm.ask_order_size.clone();
            let m_price = cm.match_price.clone();
            let m_size = cm.match_size.clone();
            let is_active = cm.is_active.clone();

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

            // ── Family 5b: input-completeness membership (#157) ──────────────
            // Each active leg's commitment must be a leaf of the round's
            // admitted-set Merkle root. Together with a watcher recomputing
            // that root from the public OrderPlaced log (and `admit_chain`
            // binding it into z_n), this proves every settled order was part of
            // the publicly admitted set — the operator cannot match an off-log
            // phantom order. Gated by `is_active`: padding rows carry a
            // default all-zero path and impose no constraint on the root.
            let bid_member_root = merkle_root_var(cs.clone(), &cfg, &bid_commit, &cm.bid_path)?;
            ((&bid_member_root - &external_inputs.admitted_root) * &is_active)
                .enforce_equal(&zero)?;
            let ask_member_root = merkle_root_var(cs.clone(), &cfg, &ask_commit, &cm.ask_path)?;
            ((&ask_member_root - &external_inputs.admitted_root) * &is_active)
                .enforce_equal(&zero)?;

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

            // ── Settlement-tuple chain (#153) ────────────────────────────────
            // Fold this row's plaintext settlement tuple into the running
            // chain on active rows; inactive (padding) rows pass the
            // accumulator through unchanged. `bid_addr`/`ask_addr` are the
            // settlement addresses as field elements (== uint256(address)),
            // matching what the contract hashes from `matches[]`.
            let mut settle_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
            settle_sponge.absorb(
                &[
                    settlement_acc.clone(),
                    bid_addr.clone(),
                    ask_addr.clone(),
                    m_price.clone(),
                    m_size.clone(),
                ]
                .as_ref(),
            )?;
            let settle_next = settle_sponge.squeeze_field_elements(1)?[0].clone();
            settlement_acc = &settle_next * &is_active + &settlement_acc * (&one - &is_active);

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

        // ── Admitted-set chain (#157) ───────────────────────────────────────
        // Fold this round's admitted-set root into the running chain. Mirrors
        // the native `admitted_chain_step` (poseidon(prev, root)) so a watcher
        // reproduces z_n[4] by folding each round's recomputed root in order.
        let mut admit_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        admit_sponge.absorb(&[admit_chain_prev, external_inputs.admitted_root.clone()].as_ref())?;
        let new_admit_chain = admit_sponge.squeeze_field_elements(1)?[0].clone();

        Ok(vec![
            new_state_hash,
            new_round_nonce,
            policy_hash,
            settlement_acc,
            new_admit_chain,
        ])
    }
}

/// Recompute a Merkle root from a leaf and its membership path — the in-circuit
/// mirror of `merkle::root_from_proof`. `index_bit == 1` means the current node
/// is the right child (sibling on the left), matching the native convention.
fn merkle_root_var(
    cs: ConstraintSystemRef<Fr>,
    cfg: &PoseidonConfig<Fr>,
    leaf: &FpVar<Fr>,
    path: &MerklePathVar,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut cur = leaf.clone();
    for (sib, bit) in path.siblings.iter().zip(path.index_bits.iter()) {
        let left = FpVar::conditionally_select(bit, sib, &cur)?;
        let right = FpVar::conditionally_select(bit, &cur, sib)?;
        let mut s = PoseidonSpongeVar::<Fr>::new(cs.clone(), cfg);
        s.absorb(&[left, right].as_ref())?;
        cur = s.squeeze_field_elements(1)?[0].clone();
    }
    Ok(cur)
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
    use crate::merkle::admitted_chain_step;
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
        // [state_hash, round_nonce, policy_hash, settlement_acc, admit_chain]
        vec![Fr::zero(), Fr::zero(), policy_hash, Fr::zero(), Fr::zero()]
    }

    /// Native mirror of the in-circuit settlement hash-chain (#153): folds
    /// each active match's `(bid_addr, ask_addr, price_8, size_8)` tuple into a
    /// running Poseidon chain starting from `acc0`. This is the value the
    /// on-chain `settleSession` must reproduce from `matches[]`.
    fn settlement_chain(ext: &AuctionExternalInputs, acc0: Fr) -> Fr {
        let cfg = poseidon_config();
        let mut acc = acc0;
        for m in &ext.matches {
            if m.is_active != Fr::one() {
                continue;
            }
            let mut s = PoseidonSponge::<Fr>::new(&cfg);
            s.absorb(&vec![
                acc,
                m.bid_trader_addr,
                m.ask_trader_addr,
                m.match_price,
                m.match_size,
            ]);
            acc = s.squeeze_field_elements::<Fr>(1)[0];
        }
        acc
    }

    #[test]
    fn settlement_acc_matches_native_chain() {
        let (w, prices, sizes) = two_match_witness(Decimal::from(100), Decimal::from(100));
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 3).unwrap();
        let z_0 = initial_z(&ext);
        let expected = settlement_chain(&ext, z_0[3]);
        let circuit = AuctionStepCircuit::new(3).unwrap();
        let (satisfied, z_next) = run_step(&circuit, z_0, ext);
        assert!(satisfied, "valid two-match batch must satisfy");
        assert_eq!(
            z_next[3], expected,
            "in-circuit settlement_acc must equal the native hash-chain the contract recomputes"
        );
        assert_ne!(
            z_next[3],
            Fr::zero(),
            "non-empty batch must advance the chain"
        );
    }

    /// The #153 binding property: if the operator settles a DIFFERENT set of
    /// matches than the one proved, the chain the contract recomputes over
    /// `matches[]` no longer equals the proof's `z_n[3]`, so settlement is
    /// rejected. We model the contract's recompute with `settlement_chain`
    /// over operator-supplied (tampered) tuples and assert it diverges from the
    /// proved value.
    #[test]
    fn settlement_chain_detects_match_substitution() {
        // Honest batch → the value bound into z_n[3] by the proof.
        let (w, prices, sizes) = two_match_witness(Decimal::from(100), Decimal::from(100));
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 3).unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(3).unwrap();
        let (ok, z_next) = run_step(&circuit, z_0.clone(), ext);
        assert!(ok);
        let bound = z_next[3];

        // Operator swaps a settled counterparty to a colluding address.
        let (mut w_addr, p_a, s_a) = two_match_witness(Decimal::from(100), Decimal::from(100));
        w_addr.matches[0].ask.trader_addr = "cc".repeat(20);
        let ext_addr = AuctionExternalInputs::from_witness(&w_addr, &p_a, &s_a, 3).unwrap();
        assert_ne!(
            settlement_chain(&ext_addr, z_0[3]),
            bound,
            "substituting a settled address must break the bound chain"
        );

        // Operator settles at a different price than was proved.
        let (w_px, p_x, s_x) = two_match_witness(Decimal::from(100), Decimal::from(100));
        let ext_px = AuctionExternalInputs::from_witness(
            &w_px,
            &[Decimal::from(100), Decimal::from(99)],
            &s_x,
            3,
        )
        .unwrap();
        let _ = p_x;
        assert_ne!(
            settlement_chain(&ext_px, z_0[3]),
            bound,
            "substituting a settled price must break the bound chain"
        );
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

    // ── Input-completeness membership (#157) ─────────────────────────────────

    #[test]
    fn admit_chain_advances_with_round_root() {
        let (w, prices, sizes) = sample_witness();
        let ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let root = ext.admitted_root;
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, z_next) = run_step(&circuit, z_0.clone(), ext);
        assert!(satisfied, "valid round with membership paths must satisfy");
        assert_eq!(
            z_next[4],
            admitted_chain_step(z_0[4], root),
            "z[4] must fold poseidon(prev, admitted_root) — the watcher recomputes this"
        );
        assert_ne!(
            z_next[4], z_0[4],
            "a real round must advance the admit chain"
        );
    }

    /// The engine's real case: the admitted set is the full live book, a
    /// superset of the matched legs. Membership still holds for every settled
    /// leg.
    #[test]
    fn step_accepts_admitted_superset() {
        let (w, prices, sizes) = sample_witness();
        let base = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        let mut admitted: Vec<Fr> = base
            .matches
            .iter()
            .filter(|m| m.is_active == Fr::one())
            .flat_map(|m| [leaf_of(m, true), leaf_of(m, false)])
            .collect();
        // Unrelated commitments standing in for other admitted-but-unmatched
        // orders that sit in the book this round.
        admitted.extend([Fr::from(7u64), Fr::from(99u64), Fr::from(123_456u64)]);
        let ext =
            AuctionExternalInputs::from_witness_with_admitted(&w, &prices, &sizes, 2, &admitted)
                .unwrap();
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            satisfied,
            "matched legs must be members of the larger admitted set"
        );
    }

    /// A settled order whose commitment was never admitted is rejected at
    /// witness-build time — there is no membership path to give it.
    #[test]
    fn from_witness_with_admitted_rejects_unadmitted_match() {
        let (w, prices, sizes) = sample_witness();
        let admitted = vec![Fr::from(1u64), Fr::from(2u64)]; // omits the matched legs
        let res =
            AuctionExternalInputs::from_witness_with_admitted(&w, &prices, &sizes, 2, &admitted);
        assert!(
            res.is_err(),
            "a settled order absent from the admitted set must be rejected"
        );
    }

    /// The binding property: if the operator claims a different admitted root
    /// than the one the membership paths actually prove (e.g. to hide a
    /// censored input set), the in-circuit membership check fails.
    #[test]
    fn step_rejects_tampered_admitted_root() {
        let (w, prices, sizes) = sample_witness();
        let mut ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        ext.admitted_root += Fr::from(1u64);
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "membership must fail when the bound root is not the one the paths prove"
        );
    }

    #[test]
    fn step_rejects_tampered_membership_path() {
        let (w, prices, sizes) = sample_witness();
        let mut ext = AuctionExternalInputs::from_witness(&w, &prices, &sizes, 2).unwrap();
        ext.matches[0].bid_path.siblings[0] += Fr::from(1u64);
        let z_0 = initial_z(&ext);
        let circuit = AuctionStepCircuit::new(2).unwrap();
        let (satisfied, _) = run_step(&circuit, z_0, ext);
        assert!(
            !satisfied,
            "a corrupted membership path must not verify to the bound root"
        );
    }
}
