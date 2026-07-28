# Issue #217 — Client-bound order proof (ingestion enforcement)

- **Date:** 2026-06-16
- **Issue:** [#217](https://github.com/Dnreikronos/DarkPool-Exchange/issues/217) — *No nullifier / replay binding on the order proof*
- **Amends:** [ADR-0001](../../adr/0001-client-order-proof.md) (enforcement was deferred to #97/#98)
- **Builds on:** #243 (nullifier public input, merged), #244 (ciphertext spent-set, merged)

## Problem

Before the #217 enforcement work, the per-order Groth16 proof was **accepted
unverified** at ingestion
(`Engine::place_encrypted_order`). The merged circuit (#243) now exposes a
nullifier public input (`poseidon(NULLIFIER_DOMAIN, commitment, salt)`), but
no code verified the proof or tracked a nullifier spent-set, so a captured
`(proof, commitment)` pair carries no replay/trader binding. #244 keys a
spent-set on the ciphertext SHA-256, which catches byte-identical replays but
is not a cryptographic per-order binding.

## Why the naive fix is blocked

Post-[#153], the engine derives the salt server-side from a per-boot secret
nonce + a server-assigned `order_id`
(`salt = SHA256("salt" ‖ nonce ‖ commitment_key ‖ order_id)`), so the client
cannot reproduce the commitment, and a server-derived nullifier is
non-deterministic per submission — a spent-set on it would add nothing over
[#244](https://github.com/Dnreikronos/DarkPool-Exchange/pull/244). Genuine
nullifier binding is inherently a **client-side** construct.

There is also a second divergence: the WASM prover binds `trader_id` to the
**commitment_key** (`dp-zk-wasm/src/lib.rs:31-35`), while the engine binds it to
the **on-chain address** (`derive_trader_id_bytes(order.trader)`, post-#153). So
even with a shared salt the two commitments differ.

## Decision (ADR-0001 "client-reproducible commitment" path)

Make the order commitment **client-reproducible and proof-verified at
ingestion**, binding it to the salt *and* the on-chain trader:

1. **Client-chosen salt travels inside the ciphertext.** The client already
   samples a salt (`submit.rs`); it now goes into the encrypted `OrderPayload`
   / `DecryptedOrder`. The engine uses the decrypted salt instead of deriving
   one server-side. (Partial revert of #153 — salt only; `trader_id` stays
   address-bound.)
2. **`trader_id` is bound to the on-chain address on both sides.** The client
   proves with `trader_addr = bytes_to_scalar(trader_address)` and
   `trader_id = derive_trader_id(trader_address)` — the exact derivation the
   engine already uses. The proof is thereby bound to the trader (#217's
   "bind to trader"; folds in the trader half of #97/#98).
3. **Engine re-derives `[commitment, nullifier]`** from the decrypted fields +
   client salt (operator re-derivation, #158, is retained as defense-in-depth)
   and **verifies the proof against the pinned canonical VK**
   (`verify_proof_with_vk`). The VK is loaded once at boot.
4. **Nullifier spent-set.** The engine re-derives the nullifier
   (`compute_nullifier_native`) and keys a persistent spent-set on it — a
   projection mirroring `seen_ciphertexts` (snapshot / recover-rebuild /
   reset). A repeat nullifier is rejected with `DuplicateOrder`.
5. **Round-id binding is intentionally omitted** (per #243): a global,
   persistent nullifier spent-set makes each order single-use across *all*
   rounds — strictly stronger than per-round binding — and the client cannot
   know its round at prove time.

## Threat-model delta vs #153

\#153 moved the salt server-side to stop a client choosing it. Here the client
chooses the salt again, but: (a) `trader_id` stays bound to the verified
on-chain address (the actual #153 fix — unchanged), and (b) the salt is only
blinding entropy; a client-chosen 32-byte random salt is as unguessable to
third parties as a server-derived one. The engine still re-derives the
commitment from decrypted fields and now *verifies* the proof, so a malformed
or mismatched salt fails verification rather than being silently trusted.

## PR decomposition (each compiles, tested, clippy + rustfmt clean; "Part of #217")

1. **Salt in the ciphertext.** `dp-client` (payload/submit/encrypt), `dp-crypto`
   (`DecryptedOrder`), `dp-engine` uses decrypted salt; persist salt in
   `OrderPlaced`; recovery/snapshot recompute from persisted salt; drop the
   server `derive_salt` / `salt_nonce` path.
2. **Address-bound `trader_id` in the prover.** `dp-zk-wasm` witness takes
   `trader_addr`; derive `trader_id` from it; native `dp-client` prove path;
   tests asserting engine-derived commitment == proof commitment.
3. **VK pinning + ingestion verify.** `dp-engine` loads the canonical
   commitment-circuit VK at boot; `dp-api` config for its path; verify the
   proof against `[commitment, nullifier]`; reject invalid.
4. **Nullifier spent-set.** `dp-engine` state/snapshot/recover; reject repeats.
5. **Real client proof end-to-end + cleanup.** Wire the prover output into the
   submission (remove `PLACEHOLDER_PROOF`); regenerate the v3 demo proving key;
   amend ADR-0001; update proto field comments.

## Out of scope / follow-ups

- Per-user (SIWE) auth that would let the engine reject an unauthenticated
  API-key holder forging another trader's order at the *transport* layer; the
  proof now binds the trader cryptographically, but the unauthenticated
  `caller = None` path still relies on that binding alone.
- Expiry-based pruning of the spent-sets (needs the #233 freshness token).
