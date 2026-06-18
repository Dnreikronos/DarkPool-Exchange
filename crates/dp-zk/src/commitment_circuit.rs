//! Groth16 circuit proving an order's commitment preimage is well-formed.
//!
//! Public inputs: commitment, nullifier (2 Fr elements, in that order)
//! Private witness: (trader_id, trader_addr, side, limit_price, size, salt)
//! Constraints (ADR-0001 §2, mirroring the IVC step circuit's families):
//!   - family 5: `poseidon(trader_id, side, limit_price, size, salt) == commitment`
//!   - family 9: `poseidon(trader_addr) == trader_id`   (identity binding)
//!   - family 1: `side ∈ {0, 1}`
//!   - family 4: `limit_price < 2^60`, `size < 2^60`
//!   - #217:     `poseidon(NULLIFIER_DOMAIN, commitment, salt) == nullifier`
//!
//! Without families 1/4/9 a prover could commit to `side = 7`, a negative or
//! overflowing price/size, or an arbitrary `trader_id` and still verify (#216).
//! The nullifier (#217) is a public, per-order uniqueness token bound to the
//! secret `salt`, so the engine can track a spent-set keyed on it rather than on
//! the re-randomizable Groth16 proof bytes — which carry no replay binding.

use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::snark::SNARK;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::constraints::PoseidonSpongeVar;
use ark_crypto_primitives::sponge::poseidon::PoseidonSponge;
use ark_crypto_primitives::sponge::CryptographicSponge;
use ark_groth16::Groth16;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::boolean::Boolean;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::fields::FieldVar;
use ark_r1cs_std::prelude::ToBitsGadget;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use ark_std::rand::{CryptoRng, RngCore};

use crate::pedersen::{commit_native, poseidon_config, OrderCommitmentInput};

#[derive(Clone, Debug)]
pub struct CommitmentPreimageCircuit {
    pub trader_id: Fr,
    /// Preimage of `trader_id`: the commitment-key scalar, with
    /// `trader_id == poseidon(trader_addr)` enforced in-circuit (ADR-0001 §2,
    /// family 9). This proves `trader_id` is a well-formed Poseidon image —
    /// the same shape the engine and step circuit derive — so the commitment
    /// can later flow through matching. `trader_addr` is a private witness and
    /// `commitment` the sole public input, so binding the proof to a *specific*
    /// verified caller needs `trader_addr` (or the caller address) exposed as a
    /// public input and checked at ingestion: deferred to #97/#98.
    pub trader_addr: Fr,
    pub side: Fr,
    pub limit_price: Fr,
    pub size: Fr,
    pub salt: Fr,
}

impl ConstraintSynthesizer<Fr> for CommitmentPreimageCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let cfg = poseidon_config();

        let trader_id = FpVar::new_witness(cs.clone(), || Ok(self.trader_id))?;
        let trader_addr = FpVar::new_witness(cs.clone(), || Ok(self.trader_addr))?;
        let side = FpVar::new_witness(cs.clone(), || Ok(self.side))?;
        let limit_price = FpVar::new_witness(cs.clone(), || Ok(self.limit_price))?;
        let size = FpVar::new_witness(cs.clone(), || Ok(self.size))?;
        let salt = FpVar::new_witness(cs.clone(), || Ok(self.salt))?;

        // ── Family 1: side ∈ {0, 1} ─────────────────────────────────────────
        let one = FpVar::<Fr>::one();
        let zero = FpVar::<Fr>::zero();
        (&side * (&one - &side)).enforce_equal(&zero)?;

        // ── Family 4: 60-bit range on price and size ────────────────────────
        // Rejects negative (field-wrapped) and overflowing magnitudes that the
        // commitment hash would otherwise launder into a valid-looking proof.
        enforce_range_60(&limit_price)?;
        enforce_range_60(&size)?;

        // ── Family 9: trader-id identity binding ────────────────────────────
        // `trader_id` must be `poseidon(trader_addr)`, mirroring `step_circuit`'s
        // family-9 gadget so the same commitment can flow through matching.
        // `trader_addr` is a private witness and `commitment` the sole public
        // input here, so this proves `trader_id` is a well-formed Poseidon image,
        // not that it belongs to a specific caller — binding to the verified
        // caller is the deferred #97/#98 work.
        let mut id_sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        id_sponge.absorb(&[trader_addr].as_ref())?;
        let derived_trader_id = id_sponge.squeeze_field_elements(1)?[0].clone();
        derived_trader_id.enforce_equal(&trader_id)?;

        // ── Family 5: commitment binding ────────────────────────────────────
        let expected = compute_commitment_native(&self);
        let expected_var = FpVar::new_input(cs.clone(), || Ok(expected))?;

        let mut sponge = PoseidonSpongeVar::<Fr>::new(cs.clone(), &cfg);
        sponge.absorb(&[trader_id, side, limit_price, size, salt.clone()].as_ref())?;
        let computed = sponge.squeeze_field_elements(1)?[0].clone();

        computed.enforce_equal(&expected_var)?;

        // ── #217: nullifier binding ─────────────────────────────────────────
        // `nullifier = poseidon(NULLIFIER_DOMAIN, commitment, salt)`, a public
        // per-order uniqueness token. Bound to the secret `salt` so the spent-set
        // keys on this value, never on the re-randomizable proof bytes. The
        // domain tag keeps it from ever colliding with the commitment hash above.
        let expected_nullifier = compute_nullifier_native(expected, self.salt);
        let nullifier_var = FpVar::new_input(cs.clone(), || Ok(expected_nullifier))?;
        let domain = FpVar::new_constant(cs.clone(), nullifier_domain())?;

        let mut null_sponge = PoseidonSpongeVar::<Fr>::new(cs, &cfg);
        null_sponge.absorb(&[domain, computed, salt].as_ref())?;
        let computed_nullifier = null_sponge.squeeze_field_elements(1)?[0].clone();

        computed_nullifier.enforce_equal(&nullifier_var)?;

        Ok(())
    }
}

fn compute_commitment_native(circuit: &CommitmentPreimageCircuit) -> Fr {
    let input = OrderCommitmentInput {
        trader_id: circuit.trader_id,
        side: circuit.side,
        limit_price: circuit.limit_price,
        size: circuit.size,
        salt: circuit.salt,
    };
    commit_native(&input)
}

/// Domain-separation tag absorbed first into the nullifier hash so it can never
/// alias the commitment hash (which omits the tag) or any other Poseidon use.
/// Fixed for the `v3` circuit; changing it changes the nullifier and therefore
/// the key material, so bump [`COMMITMENT_CIRCUIT_VERSION`] alongside it.
fn nullifier_domain() -> Fr {
    crate::pedersen::bytes_to_scalar(b"DP/order-nullifier/v1")
}

/// Native `poseidon(NULLIFIER_DOMAIN, commitment, salt)`. MUST stay byte-for-byte
/// identical to the in-circuit gadget in `generate_constraints`: the engine
/// recomputes it to cross-check the public-input nullifier and to key its replay
/// spent-set (#217).
pub fn compute_nullifier_native(commitment: Fr, salt: Fr) -> Fr {
    let cfg = poseidon_config();
    let mut sponge = PoseidonSponge::<Fr>::new(&cfg);
    sponge.absorb(&vec![nullifier_domain(), commitment, salt]);
    sponge.squeeze_field_elements::<Fr>(1)[0]
}

/// Public inputs of a [`CommitmentPreimageCircuit`] proof, in the exact order the
/// circuit allocates them. Centralising the order here keeps the prover, the
/// `verify_*` entrypoints, and the on-the-wire decode from ever disagreeing on
/// it — a mismatch would silently reject every proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderProofPublics {
    /// `poseidon(trader_id, side, limit_price, size, salt)`.
    pub commitment: Fr,
    /// `poseidon(NULLIFIER_DOMAIN, commitment, salt)` — the replay/spent-set key.
    pub nullifier: Fr,
}

impl OrderProofPublics {
    /// Derive both public inputs natively from the order's commitment and salt.
    pub fn derive(commitment: Fr, salt: Fr) -> Self {
        Self {
            commitment,
            nullifier: compute_nullifier_native(commitment, salt),
        }
    }

    /// Public-input slice in circuit-allocation order: `[commitment, nullifier]`.
    fn as_inputs(&self) -> [Fr; 2] {
        [self.commitment, self.nullifier]
    }
}

/// Range-proof width for price/size: values must fit in 60 bits. Matches the
/// encoder's `MAX_ENCODED = 2^60` and `step_circuit`'s `SIZE_BITS`.
const SIZE_BITS: usize = 60;

/// Enforce `0 <= value < 2^60` by binding every bit above bit 59 to zero. A
/// negative field element (e.g. `-1 = p-1`) has high bits set and is rejected,
/// exactly as in `step_circuit::enforce_range_60`.
fn enforce_range_60(value: &FpVar<Fr>) -> Result<(), SynthesisError> {
    let bits = value.to_bits_le()?;
    for bit in bits.iter().skip(SIZE_BITS) {
        bit.enforce_equal(&Boolean::FALSE)?;
    }
    Ok(())
}

/// Output of the demo-only [`setup_and_prove`]: bundles a prover-chosen
/// verifying key alongside the proof. Compiled only under the `fixtures`
/// feature (issue #212); the sound path ([`prove_with_key`]) emits no VK.
#[cfg(feature = "fixtures")]
pub struct SingleOrderProof {
    pub publics: OrderProofPublics,
    pub proof_bytes: Vec<u8>,
    pub vk_bytes: Vec<u8>,
}

type Groth16Keys = (
    ark_groth16::ProvingKey<Bn254>,
    ark_groth16::VerifyingKey<Bn254>,
);

/// Run the **one-time** trusted setup for `CommitmentPreimageCircuit` and
/// return the proving/verifying key pair.
///
/// This is the only sound way to obtain a verifying key: setup is run once,
/// the resulting VK is pinned as canonical, and every prover then uses the
/// matching proving key. The verifier must NEVER accept a prover-supplied
/// VK — see [`verify_proof_with_vk`].
///
/// The circuit shape is fixed (witness values don't affect the constraint
/// system here — they only affect the public-input value), so any concrete
/// circuit instance produces an interchangeable key pair for the same RNG.
pub fn generate_keys<R: RngCore + CryptoRng>(rng: &mut R) -> Result<Groth16Keys, crate::ZkError> {
    // Setup only inspects the constraint structure, which is identical for
    // every instance of this circuit, so the witness values are placeholders.
    // We still pick an internally consistent instance — `trader_id =
    // poseidon(trader_addr)` over the empty key (both reduce to 0), side a bit,
    // price/size in range — so the shape is a valid assignment, not merely a
    // structurally valid one.
    let shape = CommitmentPreimageCircuit {
        trader_id: crate::pedersen::derive_trader_id(&[]).expect("derive_trader_id is infallible"),
        trader_addr: Fr::from(0u64),
        side: Fr::from(0u64),
        limit_price: Fr::from(0u64),
        size: Fr::from(0u64),
        salt: Fr::from(0u64),
    };
    Groth16::<Bn254>::circuit_specific_setup(shape, rng)
        .map_err(|e| crate::ZkError::Setup(e.to_string()))
}

/// Prove against an existing (canonical) proving key. The verifying key is
/// NOT emitted — proving must not let the prover choose the VK. Returns the
/// proof's public inputs (commitment + nullifier; the engine cross-checks both)
/// and the compressed proof bytes.
pub fn prove_with_key<R: RngCore + CryptoRng>(
    pk: &ark_groth16::ProvingKey<Bn254>,
    circuit: &CommitmentPreimageCircuit,
    rng: &mut R,
) -> Result<(OrderProofPublics, Vec<u8>), crate::ZkError> {
    let publics = OrderProofPublics::derive(compute_commitment_native(circuit), circuit.salt);
    let proof = Groth16::<Bn254>::prove(pk, circuit.clone(), rng)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;
    let mut proof_bytes = Vec::new();
    proof
        .serialize_with_mode(&mut proof_bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok((publics, proof_bytes))
}

/// On-disk version tag for serialized [`CommitmentPreimageCircuit`] key
/// material. This is **independent** of [`crate::CIRCUIT_VERSION`] (which
/// tracks the HyperNova IVC circuit, not this Groth16 commitment circuit).
/// Bump it whenever the circuit constraints — and therefore the key material —
/// change, so a node can never silently load keys minted for an incompatible
/// circuit and discover the mismatch only when proofs start failing.
///
/// `v2` adds the ADR-0001 §2 side-bit, 60-bit range, and identity-binding
/// constraints (#216). `v3` adds the #217 nullifier public input and its binding
/// constraint, which changes the constraint system and the public-input arity —
/// again new key material. Any `v1`/`v2` keys must be regenerated.
pub const COMMITMENT_CIRCUIT_VERSION: &str = "v3-poseidon-commitment-nullifier";

/// Magic prefix identifying a versioned commitment-key envelope.
const KEY_ENVELOPE_MAGIC: &[u8; 8] = b"DPCMTKEY";

/// Wrap raw arkworks key bytes in `[magic | u32 ver_len | ver | body]`.
fn wrap_key_envelope(body: &[u8]) -> Vec<u8> {
    let ver = COMMITMENT_CIRCUIT_VERSION.as_bytes();
    let mut out = Vec::with_capacity(KEY_ENVELOPE_MAGIC.len() + 4 + ver.len() + body.len());
    out.extend_from_slice(KEY_ENVELOPE_MAGIC);
    out.extend_from_slice(&(ver.len() as u32).to_le_bytes());
    out.extend_from_slice(ver);
    out.extend_from_slice(body);
    out
}

/// Validate the envelope header and return the inner arkworks bytes. Rejects a
/// missing/garbled magic and any `COMMITMENT_CIRCUIT_VERSION` mismatch.
fn unwrap_key_envelope(bytes: &[u8]) -> Result<&[u8], crate::ZkError> {
    let header = KEY_ENVELOPE_MAGIC.len() + 4;
    if bytes.len() < header || &bytes[..KEY_ENVELOPE_MAGIC.len()] != KEY_ENVELOPE_MAGIC {
        return Err(crate::ZkError::Serialize(
            "commitment key blob: missing or invalid envelope magic (regenerate via \
             `dp-zk-cli setup-commitment-circuit`)"
                .to_string(),
        ));
    }
    let ver_len = u32::from_le_bytes(
        bytes[KEY_ENVELOPE_MAGIC.len()..header]
            .try_into()
            .expect("4-byte slice"),
    ) as usize;
    let ver_end = header + ver_len;
    if bytes.len() < ver_end {
        return Err(crate::ZkError::Serialize(
            "commitment key blob: truncated version field".to_string(),
        ));
    }
    let ver = std::str::from_utf8(&bytes[header..ver_end]).map_err(|_| {
        crate::ZkError::Serialize("commitment key blob: non-utf8 version".to_string())
    })?;
    if ver != COMMITMENT_CIRCUIT_VERSION {
        return Err(crate::ZkError::Setup(format!(
            "commitment key version mismatch: expected {COMMITMENT_CIRCUIT_VERSION}, found {ver} \
             (regenerate keys via `dp-zk-cli setup-commitment-circuit`)"
        )));
    }
    Ok(&bytes[ver_end..])
}

/// Serialize a verifying key to a versioned, compressed blob (for
/// embedding/distribution). See [`COMMITMENT_CIRCUIT_VERSION`].
pub fn serialize_vk(vk: &ark_groth16::VerifyingKey<Bn254>) -> Result<Vec<u8>, crate::ZkError> {
    let mut bytes = Vec::new();
    vk.serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok(wrap_key_envelope(&bytes))
}

/// Serialize a proving key to a versioned, compressed blob (for distribution
/// to provers). See [`COMMITMENT_CIRCUIT_VERSION`].
pub fn serialize_pk(pk: &ark_groth16::ProvingKey<Bn254>) -> Result<Vec<u8>, crate::ZkError> {
    let mut bytes = Vec::new();
    pk.serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(|e| crate::ZkError::Serialize(e.to_string()))?;
    Ok(wrap_key_envelope(&bytes))
}

/// Deserialize a verifying key from a versioned blob, rejecting any
/// `COMMITMENT_CIRCUIT_VERSION` mismatch before touching the bytes.
pub fn deserialize_vk(vk_bytes: &[u8]) -> Result<ark_groth16::VerifyingKey<Bn254>, crate::ZkError> {
    let body = unwrap_key_envelope(vk_bytes)?;
    ark_groth16::VerifyingKey::<Bn254>::deserialize_with_mode(body, Compress::Yes, Validate::Yes)
        .map_err(|e| crate::ZkError::Serialize(format!("deserialize vk: {e}")))
}

/// Deserialize a proving key from a versioned blob, rejecting any
/// `COMMITMENT_CIRCUIT_VERSION` mismatch before touching the bytes.
pub fn deserialize_pk(pk_bytes: &[u8]) -> Result<ark_groth16::ProvingKey<Bn254>, crate::ZkError> {
    let body = unwrap_key_envelope(pk_bytes)?;
    ark_groth16::ProvingKey::<Bn254>::deserialize_with_mode(body, Compress::Yes, Validate::Yes)
        .map_err(|e| crate::ZkError::Serialize(format!("deserialize pk: {e}")))
}

/// **DEMO / FIXTURE USE ONLY — NOT a verification primitive.**
///
/// Runs `circuit_specific_setup` *per proof* with the caller's RNG, so the
/// prover generates its own verifying key. A proof produced this way is
/// only meaningful when verified against the VK emitted alongside it — which
/// is exactly the unsoundness a verifier must avoid: a malicious prover can
/// run setup for a trivially-satisfiable circuit and emit a `(vk, proof)`
/// pair that verifies for an arbitrary commitment.
///
/// The engine MUST NOT use this. It exists for CLI fixtures and WASM demos
/// where there is no canonical VK to verify against. Soundness at ingestion
/// comes from [`generate_keys`] (run once) + [`prove_with_key`] +
/// [`verify_proof_with_vk`] (against the pinned canonical VK).
///
/// Compiled only under the `fixtures` feature (issue #212): the unsound,
/// prover-chosen-VK path must never link into a production build.
#[cfg(feature = "fixtures")]
pub fn setup_and_prove<R: RngCore + CryptoRng>(
    circuit: &CommitmentPreimageCircuit,
    rng: &mut R,
) -> Result<SingleOrderProof, crate::ZkError> {
    let (pk, vk) = generate_keys(rng)?;
    let (publics, proof_bytes) = prove_with_key(&pk, circuit, rng)?;
    let vk_bytes = serialize_vk(&vk)?;

    // Self-check the proof verifies under the just-generated VK. This is a
    // sanity check on the prover, NOT a security guarantee — the VK is
    // prover-chosen here.
    if !verify_proof(&vk_bytes, &proof_bytes, &publics)? {
        return Err(crate::ZkError::Verify);
    }

    Ok(SingleOrderProof {
        publics,
        proof_bytes,
        vk_bytes,
    })
}

/// Verify a proof against a deserialized, **canonical** verifying key. This
/// is the sound entrypoint: the VK is pinned by the verifier (loaded once at
/// boot), never supplied by the prover. The proof is bound to both public
/// inputs — `commitment` and `nullifier` — supplied via [`OrderProofPublics`].
/// The verifier derives the expected nullifier itself (the salt is secret) and
/// passes the value it expects, so a forged or mismatched nullifier fails here.
pub fn verify_proof_with_vk(
    vk: &ark_groth16::VerifyingKey<Bn254>,
    proof_bytes: &[u8],
    publics: &OrderProofPublics,
) -> Result<bool, crate::ZkError> {
    let pvk = ark_groth16::prepare_verifying_key(vk);
    verify_proof_with_processed_vk(&pvk, proof_bytes, publics)
}

/// Verify a proof against a preprocessed canonical verifying key.
///
/// Use this in hot paths that verify many orders against the same key. The
/// public inputs are still supplied by the caller in circuit order.
pub fn verify_proof_with_processed_vk(
    pvk: &ark_groth16::PreparedVerifyingKey<Bn254>,
    proof_bytes: &[u8],
    publics: &OrderProofPublics,
) -> Result<bool, crate::ZkError> {
    let proof = ark_groth16::Proof::<Bn254>::deserialize_with_mode(
        proof_bytes,
        Compress::Yes,
        Validate::Yes,
    )
    .map_err(|e| crate::ZkError::Serialize(format!("deserialize proof: {e}")))?;

    let valid = Groth16::<Bn254>::verify_with_processed_vk(pvk, &publics.as_inputs(), &proof)
        .map_err(|e| crate::ZkError::Prove(e.to_string()))?;

    Ok(valid)
}

/// Verify a proof against a verifying key supplied as bytes.
///
/// SECURITY: the caller is responsible for the provenance of `vk_bytes`. If
/// the bytes came from the prover, the result is meaningless (see the
/// fixtures-only `setup_and_prove`). Pass the canonical VK only. The engine uses
/// [`verify_proof_with_vk`] with a VK it loaded at boot.
pub fn verify_proof(
    vk_bytes: &[u8],
    proof_bytes: &[u8],
    publics: &OrderProofPublics,
) -> Result<bool, crate::ZkError> {
    let vk = deserialize_vk(vk_bytes)?;
    verify_proof_with_vk(&vk, proof_bytes, publics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::decimal_to_scalar;
    use crate::pedersen::bytes_to_scalar;
    use ark_ff::{One, Zero};
    use ark_relations::gr1cs::ConstraintSystem;
    use ark_std::rand::rngs::StdRng;
    use ark_std::rand::SeedableRng;
    use rust_decimal::Decimal;

    fn fixed_rng() -> StdRng {
        StdRng::from_seed([42u8; 32])
    }

    fn sample_circuit() -> CommitmentPreimageCircuit {
        let trader_id = crate::pedersen::derive_trader_id(b"alice").unwrap();
        let trader_addr = bytes_to_scalar(b"alice");
        let salt = bytes_to_scalar(&[0x22u8; 32]);
        CommitmentPreimageCircuit {
            trader_id,
            trader_addr,
            side: Fr::zero(),
            limit_price: decimal_to_scalar(Decimal::from(100)).unwrap(),
            size: decimal_to_scalar(Decimal::from(10)).unwrap(),
            salt,
        }
    }

    /// Synthesize the circuit into a fresh constraint system and report whether
    /// the witness satisfies every constraint. The commitment public input is
    /// derived inside `generate_constraints` from the (possibly tampered)
    /// witness, so a forged field still satisfies the commitment-binding family
    /// — these tests therefore isolate the side-bit, range, and identity
    /// families that #216 adds.
    fn constraints_satisfied(circuit: CommitmentPreimageCircuit) -> bool {
        let cs = ConstraintSystem::<Fr>::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        cs.is_satisfied().unwrap()
    }

    #[test]
    fn well_formed_order_satisfies_constraints() {
        assert!(constraints_satisfied(sample_circuit()));
    }

    /// The other valid `side` bit: a sell order (`side = 1`) must satisfy too,
    /// so the family-1 check accepts both bits, not just the `sample_circuit`
    /// buy side.
    #[test]
    fn well_formed_sell_side_satisfies_constraints() {
        let mut c = sample_circuit();
        c.side = Fr::one();
        assert!(constraints_satisfied(c));
    }

    /// #216: `side` must be a bit; `side = 7` previously slipped through.
    #[test]
    fn rejects_non_bit_side() {
        let mut c = sample_circuit();
        c.side = Fr::from(7u64);
        assert!(!constraints_satisfied(c));
    }

    /// #216: price at the 2^60 boundary overflows the 60-bit range check.
    #[test]
    fn rejects_oversized_price() {
        let mut c = sample_circuit();
        c.limit_price = Fr::from(1u128 << 60);
        assert!(!constraints_satisfied(c));
    }

    /// #216: size at the 2^60 boundary overflows the 60-bit range check.
    #[test]
    fn rejects_oversized_size() {
        let mut c = sample_circuit();
        c.size = Fr::from(1u128 << 60);
        assert!(!constraints_satisfied(c));
    }

    /// #216: a negative amount wraps to `p - 1`, whose high bits are set, so
    /// the range check rejects it.
    #[test]
    fn rejects_negative_price() {
        let mut c = sample_circuit();
        c.limit_price = -Fr::one();
        assert!(!constraints_satisfied(c));
    }

    /// #216: `trader_id` must equal `poseidon(trader_addr)`; an arbitrary
    /// `trader_id` no longer verifies.
    #[test]
    fn rejects_forged_trader_id() {
        let mut c = sample_circuit();
        c.trader_id += Fr::one();
        assert!(!constraints_satisfied(c));
    }

    /// The mirror of the above: tampering with the preimage breaks the binding.
    #[test]
    fn rejects_forged_trader_addr() {
        let mut c = sample_circuit();
        c.trader_addr += Fr::one();
        assert!(!constraints_satisfied(c));
    }

    /// #217: the nullifier is a real, enforced public input — a proof verifies
    /// only against the nullifier the circuit actually bound. An attacker cannot
    /// recompute it (the salt is secret), and swapping in any other value is
    /// rejected. This is what lets the engine treat the nullifier (not the
    /// re-randomizable proof bytes) as the replay/spent-set key.
    #[test]
    fn rejects_tampered_nullifier() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();
        let (publics, proof_bytes) = prove_with_key(&pk, &sample_circuit(), &mut rng).unwrap();

        assert!(verify_proof_with_vk(&vk, &proof_bytes, &publics).unwrap());

        let mut tampered = publics;
        tampered.nullifier += Fr::one();
        assert!(!verify_proof_with_vk(&vk, &proof_bytes, &tampered).unwrap());
    }

    /// #217: the nullifier is bound to the secret salt, so two orders identical
    /// in every field but the salt produce distinct nullifiers — the spent-set
    /// key is per-order, not per-(price, size, side).
    #[test]
    fn nullifier_changes_with_salt() {
        let mut rng = fixed_rng();
        let (pk, _vk) = generate_keys(&mut rng).unwrap();

        let mut a = sample_circuit();
        a.salt = bytes_to_scalar(&[0x01u8; 32]);
        let mut b = sample_circuit();
        b.salt = bytes_to_scalar(&[0x02u8; 32]);

        let (pa, _) = prove_with_key(&pk, &a, &mut rng).unwrap();
        let (pb, _) = prove_with_key(&pk, &b, &mut rng).unwrap();
        assert_ne!(pa.nullifier, pb.nullifier);
        assert_ne!(pa.commitment, pb.commitment);
    }

    /// #217: a replayed order — same secrets, re-proven — reproduces the SAME
    /// nullifier, which is exactly what lets a spent-set reject the replay. The
    /// proof bytes, by contrast, differ run-to-run (Groth16 proving is
    /// randomized), underscoring why they cannot be the uniqueness token.
    #[test]
    fn replayed_order_reuses_nullifier() {
        let mut rng = fixed_rng();
        let (pk, _vk) = generate_keys(&mut rng).unwrap();
        let circuit = sample_circuit();

        let (p1, b1) = prove_with_key(&pk, &circuit, &mut rng).unwrap();
        let (p2, b2) = prove_with_key(&pk, &circuit, &mut rng).unwrap();

        assert_eq!(p1, p2, "same order must reproduce the same public inputs");
        assert_ne!(b1, b2, "Groth16 proofs are re-randomized per proving");
    }

    // Demo path (prover-chosen VK): fixtures-only since issue #212.
    #[cfg(feature = "fixtures")]
    #[test]
    fn proof_verifies() {
        let circuit = sample_circuit();
        let mut rng = fixed_rng();
        let result = setup_and_prove(&circuit, &mut rng).unwrap();
        assert!(verify_proof(&result.vk_bytes, &result.proof_bytes, &result.publics).unwrap());
    }

    #[cfg(feature = "fixtures")]
    #[test]
    fn proof_rejects_wrong_commitment() {
        let circuit = sample_circuit();
        let mut rng = fixed_rng();
        let result = setup_and_prove(&circuit, &mut rng).unwrap();
        let mut wrong = result.publics;
        wrong.commitment += Fr::one();
        assert!(!verify_proof(&result.vk_bytes, &result.proof_bytes, &wrong).unwrap());
    }

    #[cfg(feature = "fixtures")]
    #[test]
    fn deterministic_with_same_rng_seed() {
        let circuit = sample_circuit();
        let mut rng1 = fixed_rng();
        let mut rng2 = fixed_rng();
        let r1 = setup_and_prove(&circuit, &mut rng1).unwrap();
        let r2 = setup_and_prove(&circuit, &mut rng2).unwrap();
        assert_eq!(r1.publics, r2.publics);
        assert_eq!(r1.proof_bytes, r2.proof_bytes);
        assert_eq!(r1.vk_bytes, r2.vk_bytes);
    }

    /// A VK generated once by `generate_keys` (independent of any order's
    /// witness, using a zero shape) verifies a real order proof produced
    /// against the matching proving key. This is the canonical-VK flow the
    /// engine uses.
    #[test]
    fn canonical_keys_prove_and_verify() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();

        let circuit = sample_circuit();
        let (publics, proof_bytes) = prove_with_key(&pk, &circuit, &mut rng).unwrap();

        assert!(verify_proof_with_vk(&vk, &proof_bytes, &publics).unwrap());
        // Tampering with the commitment public input must fail.
        let mut wrong = publics;
        wrong.commitment += Fr::one();
        assert!(!verify_proof_with_vk(&vk, &proof_bytes, &wrong).unwrap());
    }

    /// SOUNDNESS REGRESSION (issues #158, #212): a proof minted under one
    /// (proving, verifying) key pair must NOT verify against a *different*
    /// canonical verifying key. This is the exact hole the old per-proof
    /// `setup_and_prove` opened by letting a prover ship a self-chosen VK.
    /// We stand in for that attacker with a second, independently generated
    /// canonical keypair — no demo helper needed, so the regression runs in
    /// the default feature set.
    #[test]
    fn proof_under_foreign_vk_rejected_by_canonical_vk() {
        let circuit = sample_circuit();

        // "Attacker" keypair: an independent setup the prover fully controls.
        let mut attacker_rng = StdRng::from_seed([7u8; 32]);
        let (attacker_pk, attacker_vk) = generate_keys(&mut attacker_rng).unwrap();
        let (publics, proof_bytes) =
            prove_with_key(&attacker_pk, &circuit, &mut attacker_rng).unwrap();

        // Operator's canonical keypair: generated once, with a different RNG.
        let mut canonical_rng = StdRng::from_seed([99u8; 32]);
        let (_pk, canonical_vk) = generate_keys(&mut canonical_rng).unwrap();

        // The proof verifies under the attacker's own VK (that's the trap)...
        assert!(verify_proof_with_vk(&attacker_vk, &proof_bytes, &publics).unwrap());
        // ...but is rejected by the pinned canonical VK.
        assert!(!verify_proof_with_vk(&canonical_vk, &proof_bytes, &publics).unwrap());
    }

    #[test]
    fn vk_pk_round_trip_serialization() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();
        let vk_bytes = serialize_vk(&vk).unwrap();
        let pk_bytes = serialize_pk(&pk).unwrap();

        let vk2 = deserialize_vk(&vk_bytes).unwrap();
        let pk2 = deserialize_pk(&pk_bytes).unwrap();

        // A proof under the round-tripped pk verifies under the round-tripped vk.
        let circuit = sample_circuit();
        let (publics, proof_bytes) = prove_with_key(&pk2, &circuit, &mut rng).unwrap();
        assert!(verify_proof_with_vk(&vk2, &proof_bytes, &publics).unwrap());
    }

    #[test]
    fn serialized_keys_carry_version_envelope() {
        let mut rng = fixed_rng();
        let (pk, vk) = generate_keys(&mut rng).unwrap();
        for blob in [serialize_vk(&vk).unwrap(), serialize_pk(&pk).unwrap()] {
            assert_eq!(&blob[..KEY_ENVELOPE_MAGIC.len()], KEY_ENVELOPE_MAGIC);
            assert!(unwrap_key_envelope(&blob).is_ok());
        }
    }

    #[test]
    fn deserialize_rejects_unversioned_blob() {
        let mut rng = fixed_rng();
        let (_, vk) = generate_keys(&mut rng).unwrap();
        // Raw arkworks bytes with no envelope must be rejected.
        let mut raw = Vec::new();
        vk.serialize_with_mode(&mut raw, Compress::Yes).unwrap();
        assert!(deserialize_vk(&raw).is_err());
    }

    #[test]
    fn deserialize_rejects_version_mismatch() {
        let mut rng = fixed_rng();
        let (_, vk) = generate_keys(&mut rng).unwrap();
        let mut body = Vec::new();
        vk.serialize_with_mode(&mut body, Compress::Yes).unwrap();
        // Hand-build an envelope with a bogus version tag.
        let ver = b"v0-bogus";
        let mut blob = Vec::new();
        blob.extend_from_slice(KEY_ENVELOPE_MAGIC);
        blob.extend_from_slice(&(ver.len() as u32).to_le_bytes());
        blob.extend_from_slice(ver);
        blob.extend_from_slice(&body);
        assert!(deserialize_vk(&blob).is_err());
    }
}
