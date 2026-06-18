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

The legacy native `dp-client` path still emits a development placeholder proof
(`b"dp-client-v0"`). Production ingestion now rejects that placeholder unless
the operator explicitly starts the server with
`DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true`.

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

**Public inputs** (2 field elements):

| # | Name       | Description                                                                  |
|---|------------|------------------------------------------------------------------------------|
| 0 | commitment | `poseidon(trader_id, side, limit_price, order_size, salt)`                   |
| 1 | nullifier  | `poseidon(NULLIFIER_DOMAIN, commitment, salt)` - replay prevention           |

`trader_id` and `side` are private witness values embedded in the commitment.
The circuit also proves `trader_id == poseidon(trader_addr)` privately, so the
public input surface stays to `[commitment, nullifier]`.

**Witness** (private, 6 field elements):

| Name        | Description                                  |
|-------------|----------------------------------------------|
| trader_id   | Poseidon trader id committed into the order  |
| trader_addr | Trader address scalar used to derive id      |
| side        | 0 or 1                                       |
| limit_price | `decimal_to_scalar(price)` - 1e8 scaled      |
| order_size  | `decimal_to_scalar(size)` - 1e8 scaled       |
| salt        | `Fr::from_be_bytes_mod_order(salt_bytes)`    |

**Constraints** (5 families):

1. **Trader ID binding** (batch family 9):
   `poseidon(trader_addr) == trader_id`
2. **Commitment binding** (batch family 5):
   `poseidon(trader_id, side, limit_price, order_size, salt) == commitment`
3. **Side bit** (batch family 1):
   `side * (1 - side) == 0`
4. **60-bit range checks** (batch family 4, partial):
   `limit_price < 2^60`, `order_size < 2^60`
5. **Nullifier binding**:
   `poseidon(NULLIFIER_DOMAIN, commitment, salt) == nullifier`

Families the client **cannot** prove (need oracle/match data): opposite sides
(2), price crossing (3), notional (6), solvency (7), position limits (8).
These stay batch-only.

The nullifier prevents commitment replay across orders without revealing the
salt. The fixed domain tag keeps it separate from the commitment hash.

**Estimated prove time:** 1–3 s in WASM on commodity hardware. ~1200
constraints is small — two Poseidon hashes (~250 constraints each), two 60-bit
decompositions (~120 each), bit check, equality gates.

### 3. WASM proving entrypoint

The browser prover is exposed by `dp-zk-wasm` and consumed by
`front/lib/prover/prover.worker.ts`:

```rust
#[wasm_bindgen]
pub fn prove_order_wasm(
    witness_json: String,
    pk_bytes: &[u8],
) -> Vec<u8>
```

The return payload is `[proof_len u32 LE | commitment_len u32 LE | proof |
commitment]`. The native `dp-client` crate is still a local-development helper
and does not mint this production proof.

Poseidon params must match `dp-client/src/commitment.rs` exactly: rate=2,
capacity=1, alpha=5, 8 full + 57 partial rounds.

### 4. Engine-side verification

**`dp-api/src/validation.rs` - `validate_place_order`:**

Keep the cheap syntactic checks: commitment length, proof non-empty, proof <=
64 KiB, encrypted payload non-empty. The API process also loads the canonical
order-proof VK at boot and installs a verifier on the engine.

**`dp-engine/src/engine.rs` - `place_encrypted_order`:**

After decryption, derive the canonical commitment and nullifier from plaintext
fields and salt, then verify the Groth16 proof against `[commitment,
nullifier]`. The engine rejects invalid proofs before appending `OrderPlaced`.

### 5. Wire format changes

`PlaceOrderRequest` stays:

```protobuf
bytes commitment = 1;
bytes proof = 2;
bytes encrypted_payload = 3;
```

The proof's public inputs are not encoded inside Groth16 proof bytes.
`ark-groth16` receives them as a separate field slice. The engine derives the
two public inputs from the decrypted payload instead of trusting request fields.

### 6. Proving key generation and distribution

- **CLI:** `dp-zk-cli setup-commitment-circuit --out <dir>` generates `(pk, vk)`.
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

- Production servers require `DARKPOOL_ORDER_PROOF_VK` unless explicitly
  started with `DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true`.
- `PLACEHOLDER_PROOF` remains only for local development and unsafe unverified
  servers.
- Proto does not gain `trader_id` or `salt` request fields; the engine derives
  proof publics after decrypting the payload.
- Snapshots use an envelope version that includes `spent_nullifiers`; older
  snapshots are rejected and rebuilt from events or fail closed if logs were
  truncated.
- Frontend must load the proving key asynchronously and show proof-generation
  progress during order submission.

## Implementation sequencing

1. Circuit definition + keygen CLI (`dp-zk`, `dp-zk-cli`)
2. WASM prover (`dp-zk-wasm`, consumed by the frontend)
3. Engine verifier (Groth16 verify after decryption)
4. API boot config for canonical VK
5. Native `dp-client` proof wiring remains a follow-up; until then it is a
   development helper only.
