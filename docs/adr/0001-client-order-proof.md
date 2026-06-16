# ADR 0001 — Client-side order proof circuit

- **Status:** Implemented for commitment/nullifier ingestion verification
- **Date:** 2026-05-24
- **Tag:** C5
- **Issue:** [#85](https://github.com/Dnreikronos/DarkPool-Exchange/issues/85)
- **Amended:** 2026-06-16 — see "Implementation status" below ([#217](https://github.com/Dnreikronos/DarkPool-Exchange/issues/217))

## Implementation status (issues #158, #217)

The live ingestion path (`Engine::place_encrypted_order`) now decrypts the
order, re-derives the canonical Poseidon commitment plus nullifier from the
plaintext fields and client salt, and verifies the supplied Groth16 proof
against those engine-derived public inputs when a canonical verifier is
installed. `darkpool-server` fails closed unless `DARKPOOL_ORDER_PROOF_VK`
points to `commitment_vk.bin` or the operator explicitly enables the local/dev
`DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS` escape hatch.

The nullifier is tracked in a persistent spent-set, rebuilt from `OrderPlaced`
events during recovery and captured in snapshots. A replayed proof/order pair
is rejected by nullifier even when ciphertext bytes differ.

The implementation uses the client-reproducible commitment path: the client
chooses the salt inside the encrypted payload, the prover binds `trader_id` to
the on-chain trader address, and the engine derives `[commitment, nullifier]`
from decrypted fields before verification.

## Context

The in-browser client currently sends a placeholder proof (`b"dp-client-v0"`,
`crates/dp-client/src/submit.rs:23`) with every order. The API validates only
that the proof field is non-empty and under 64 KiB
(`crates/dp-api/src/validation.rs:28`). No cryptographic verification happens
at ingestion.

The batch circuit (HyperNova IVC, operator-side) proves all 9 constraint
families post-matching, but there is a window between order submission and
batch inclusion where the operator could substitute or tamper with orders
undetected. A per-order client proof closes that window: the client proves
order well-formedness before submission, and the engine verifies it at
ingestion.

This ADR is the blocker for I2.8/I2.9 (WASM prover). It defines the circuit,
the WASM entrypoint, and the engine-side verification check.

## Decision

### 1. Per-order Groth16, verified at ingestion, NOT folded into IVC

Standalone Groth16 over BN254, verified by the engine when the order arrives.
Separate from the batch HyperNova IVC chain — different purpose
(anti-manipulation at submission vs settlement-grade correctness at batch).

`ark-groth16` is already vendored at `vendor/ark-groth16/`, patched in the
workspace `Cargo.toml:83`.

### 2. Client circuit: `OrderWellFormednessCircuit`

**Public inputs** (3 field elements):

| # | Name       | Description                                                        |
|---|------------|--------------------------------------------------------------------|
| 0 | commitment | `poseidon(trader_id, side, limit_price, order_size, salt)`         |
| 1 | trader_id  | `poseidon(commitment_key_scalar)`                                  |
| 2 | nullifier  | `poseidon(commitment, salt)` — replay prevention                   |

`side` is embedded in the commitment; extracting it as a separate public input
is unnecessary (the batch circuit re-checks it).

**Witness** (private, 5 field elements):

| Name                  | Description                                       |
|-----------------------|---------------------------------------------------|
| commitment_key_scalar | `Fr::from_be_bytes_mod_order(commitment_key)`     |
| side                  | 0 or 1                                            |
| limit_price           | `decimal_to_scalar(price)` — 1e8 scaled           |
| order_size            | `decimal_to_scalar(size)` — 1e8 scaled            |
| salt                  | `Fr::from_be_bytes_mod_order(salt_bytes)`          |

**Constraints** (4 families, ~1200 R1CS constraints):

1. **Trader ID binding** (batch family 9):
   `poseidon(commitment_key_scalar) == trader_id`
2. **Commitment binding** (batch family 5):
   `poseidon(trader_id, side, limit_price, order_size, salt) == commitment`
3. **Side bit** (batch family 1):
   `side * (1 - side) == 0`
4. **60-bit range checks** (batch family 4, partial):
   `limit_price < 2^60`, `order_size < 2^60`

Families the client **cannot** prove (need oracle/match data): opposite sides
(2), price crossing (3), notional (6), solvency (7), position limits (8).
These stay batch-only.

The nullifier (`poseidon(commitment, salt)`) prevents commitment replay across
orders without revealing the salt.

**Estimated prove time:** 1–3 s in WASM on commodity hardware. ~1200
constraints is small — two Poseidon hashes (~250 constraints each), two 60-bit
decompositions (~120 each), bit check, equality gates.

### 3. WASM entrypoint in `dp-client`

New function in `crates/dp-client/src/wasm.rs`:

```rust
#[wasm_bindgen]
pub fn prove_order_wasm(
    commitment_key: &str,
    side: u8,
    price: &str,
    size: &str,
    salt_hex: &str,
    proving_key_bytes: &[u8],
) -> Result<JsValue, JsError>
```

Returns `{ proof, public_inputs, commitment, trader_id, nullifier }` (all hex
strings).

The circuit lives in a **new `crates/dp-client-zk` crate**, NOT in `dp-zk` or
`dp-client`. Reason: `dp-client` targets WASM; `dp-zk` pulls
HyperNova/Grumpkin/rayon — too heavy for the browser. Separate crate keeps
deps clean: `dp-client` depends on `dp-client-zk` only via a `wasm` feature
flag. `dp-client-zk` depends on `ark-groth16` (no `parallel` feature),
`ark-bn254`, `ark-r1cs-std`, `ark-relations`, `ark-crypto-primitives`
(Poseidon gadgets).

Poseidon params must match `dp-client/src/commitment.rs` exactly: rate=2,
capacity=1, alpha=5, 8 full + 57 partial rounds.

### 4. Engine-side verification

**`dp-api/src/validation.rs` — `validate_place_order`:**

Replace the current non-crypto check (proof non-empty + ≤ 64 KiB) with
Groth16 verification against a static VK. Deserialize proof bytes → verify
against the 3 public inputs extracted from the request fields.

**`dp-engine/src/engine.rs` — `place_encrypted_order` (~line 463):**

After decryption, cross-check: `trader_id` from the proof's public inputs MUST
equal `poseidon(commitment_key)` derived from the decrypted payload. Closes the
attack where a valid proof for trader A wraps ciphertext for trader B.

### 5. Wire format changes

`PlaceOrderRequest` in `darkpool.proto:88` adds:

```protobuf
bytes trader_id = 4;  // 32 bytes, poseidon(commitment_key)
bytes salt      = 5;  // 32 bytes
```

These are already transmitted in the REST JSON path (`OrderSubmission` struct
in `submit.rs:28`) but not in the proto. The proof's public inputs are **not**
encoded inside Groth16 proof bytes; `ark-groth16::verify_proof` receives them
as a separate `&[E::ScalarField]` slice via `prepare_inputs`. The verifier
must derive the 3 public inputs (`commitment`, `trader_id`, `nullifier`) from
the request fields and pass them alongside the deserialized proof.

### 6. Proving key generation and distribution

- **CLI:** `dp-zk-cli setup-client-circuit --out <dir>` generates `(pk, vk)`.
- **PK (~100–200 KB):** static asset hosted at CDN, loaded by the frontend
  before first order submission.
- **VK (~1 KB):** embedded in the engine binary or loaded from config at boot.
- Separate `CLIENT_CIRCUIT_VERSION` constant (no existing circuit version
  constant to collide with — the IVC circuit tracks version independently).

### 7. Batch circuit unchanged

`AuctionStepCircuit` in `dp-zk/src/step_circuit.rs` keeps all 9 constraint
families. The client proof is defense-in-depth at ingestion; the batch proof is
settlement-grade. Redundancy is intentional.

## Alternatives considered

| Option                                        | Why rejected                                                                                                                                |
|-----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------|
| **B: Formalized no-proof (status quo)**       | Leaves operator substitution window open between submission and batching.                                                                   |
| **C: Heavy client proof (solvency self-attestation)** | Client can't prove solvency without oracle; larger circuit (5–30 s WASM) for no security gain.                                      |
| **D: Plonk/KZG (no trusted setup)**          | Larger proofs (~500 B vs ~192 B), less mature arkworks support, SRS still needed; revisit if ceremony is problematic.                       |
| **E: Client proof folded into IVC**           | Client needs HyperNova params (~MB), 10–30x slower WASM proving, operator must validate each fold step.                                    |

## Consequences

- `dp-client` gains `ark-groth16` dep (WASM-compat, no rayon); bundle
  +200–400 KB.
- `PLACEHOLDER_PROOF` in `submit.rs:23` becomes dead code — remove.
- Proto gains `trader_id` + `salt` fields on `PlaceOrderRequest`.
- One-time trusted setup ceremony required before enforcement.
- Migration period: engine accepts both placeholder and real proofs
  (VK-presence check) until ceremony completes and clients upgrade.
- Frontend must load PK async and show 1–3 s proving spinner during
  order submission.

## Implementation sequencing

1. Circuit definition + keygen CLI (`dp-client-zk` crate, `dp-zk-cli` command)
2. WASM prover (`prove_order_wasm` in `dp-client`)
3. Engine verifier (Groth16 verify in `validate_place_order`)
4. Wire format (proto + REST DTO updates)
5. Ceremony → publish PK → embed VK → enforce (reject placeholders)
