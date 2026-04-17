# Handoff: Align Dark Pool to README Privacy Spec

> **Source**: reviewer thread (2026-04-17). Attach this file as context to the implementation agent. Execute phases in order. Verify `go build ./...` + `go test ./...` green between phases.

---

## Problem

Current `PlaceOrderRequest` (see `server/api/proto/darkpool/v1/darkpool.proto`) ships plaintext `price` / `size` / `side` / `pair` alongside `encrypted_payload`. This breaks the README privacy model:

- **TLS terminator, load balancer, gRPC access logs, APM traces, panic dumps** all see the full order.
- **Event store** persists decrypted `model.Order` forever — single insider leak = full order-book history.
- **`encrypted_payload` is theater**: attacker ignores the ciphertext and reads the cleartext sibling fields.
- **`proof` is not bound to the commitment**: engine accepts plaintext inputs directly; a client can send a proof of order A and plaintext of order B, and the engine will never notice. This is the bug class that kills dark pools.

## Goal

Wire carries only `{commitment, proof, encrypted_payload}`. Plaintext exists only in engine RAM during the auction window. Event log is ciphertext + commitment + proof — never plaintext.

## Constraints

- **Multi-file refactor**. Break into 3 phases (below). Verify (`go build ./... && go test ./...`) between phases.
- **No real crypto yet**. Use passthrough stubs so the pipeline runs end-to-end without keys/circuits. Real Pedersen / ECIES / halo2 are deferred to later phases.
- **Preserve crash-recovery guarantees** already tested in `server/engine/core/batch_test.go` — especially `TestBatchLifecycle_ProofPersistedAndReusedOnResubmit`. Recovery must still reuse persisted aggregated proofs.
- Follow existing seam patterns: `ProofAggregator` / `Submitter` with `Noop*` defaults and `Set*` setters (see `server/engine/core/submitter.go`, `engine.go`).

---

## Phase 1 — Engine decrypt seam (no wire change yet)

**Files**
- `server/engine/core/decrypter.go` (new)
- `server/engine/core/engine.go`
- `server/engine/core/engine_test.go`
- `server/engine/core/batch_test.go`

**Steps**

1. Create `server/engine/core/decrypter.go`:

   ```go
   type DecryptedOrder struct {
       Pair          string
       Side          utils.Side
       Price, Size   decimal.Decimal
       CommitmentKey string
       TTL           time.Duration
   }

   type Decrypter interface {
       // Decrypt takes the opaque ciphertext from the wire and returns the
       // plaintext order. Real impls use the operator private key; NoopDecrypter
       // is a passthrough that reads JSON-encoded DecryptedOrder for scaffolding.
       Decrypt(ctx context.Context, ciphertext []byte) (DecryptedOrder, error)
   }

   type NoopDecrypter struct{}

   func (NoopDecrypter) Decrypt(_ context.Context, ct []byte) (DecryptedOrder, error) {
       var out DecryptedOrder
       if err := json.Unmarshal(ct, &out); err != nil {
           return DecryptedOrder{}, fmt.Errorf("noop decrypter: %w", err)
       }
       return out, nil
   }
   ```

   Custom `UnmarshalJSON` for `DecryptedOrder` if `decimal.Decimal` / `utils.Side` don't round-trip cleanly — keep helpers in the same file.

2. `Engine` gains a `decrypter Decrypter` field. `NewEngine` defaults to `NoopDecrypter{}`. Add `SetDecrypter(d Decrypter)` mirroring `SetAggregator`.

3. Add new entrypoint on `Engine`:

   ```go
   func (e *Engine) PlaceEncryptedOrder(ctx context.Context, commitment, proof, ciphertext []byte) (*model.Order, error)
   ```

   - Call `e.decrypter.Decrypt(ctx, ciphertext)`.
   - Validate commitment binds the decrypted fields. Stub: `sha256(canonicalBytes(decrypted)) == commitment`. Leave a `TODO(zk-pipeline): replace sha256 with Pedersen once circuit lands` comment.
   - On mismatch: return a new sentinel error (e.g. `utils.ErrCommitmentMismatch`) — add to `server/engine/utils/errors.go`.
   - On success: delegate to existing `PlaceOrder` internals with decrypted fields (extract the core into an unexported helper if needed to avoid duplicating validation).

4. Keep the existing `PlaceOrder(pair, side, price, size, commitmentKey, ttl, encryptedPayload)` signature **intact** for this phase. Handler, existing tests, and `batch_test.go` stay working.

5. Add unit tests:
   - `TestPlaceEncryptedOrder_NoopRoundTrip` — encode a `DecryptedOrder`, submit, assert order lands.
   - `TestPlaceEncryptedOrder_CommitmentMismatch` — commitment bytes that don't match decrypted payload → `ErrCommitmentMismatch`.
   - `TestPlaceEncryptedOrder_BadCiphertext` — garbage bytes → error surfaces from decrypter.

6. **Gate**: `go build ./... && go test ./...` — both green. Commit as "Phase 1: add engine decrypt seam".

---

## Phase 2 — Proto flip + handler rewrite

**Files**
- `server/api/proto/darkpool/v1/darkpool.proto`
- `server/api/gen/darkpool/v1/*` (regenerated)
- `server/api/handler/handler.go`
- `server/api/handler/handler_test.go`
- `server/api/utils/errors.go`

**Steps**

1. Rewrite `PlaceOrderRequest`:

   ```proto
   message PlaceOrderRequest {
     bytes commitment        = 1; // binds proof to ciphertext
     bytes proof             = 2; // aggregated or per-order ZK proof
     bytes encrypted_payload = 3; // opaque ciphertext to operator pubkey
   }
   ```

   Remove `pair`, `side`, `price`, `size`, `commitment_key`, `ttl_seconds` from the request. Keep the response shape.

2. Regenerate protos:
   - Untracked `server/api/proto/buf.gen.yaml` exists — confirm it is wired and checked in.
   - Run `buf generate` (or whatever the repo convention is; check `Makefile` / scripts).
   - Commit regenerated `gen/` separately from handwritten diff for review clarity.

3. `server/api/utils/errors.go`:
   - Add `MsgCommitmentRequired`, `MsgCiphertextRequired`, `MsgCiphertextTooLarge`, `MsgCommitmentMismatch`.
   - Add `const MaxCiphertextBytes = 128 * 1024` (or whatever matches circuit output bound — leave TODO).

4. Rewrite `server/api/handler/handler.go :: PlaceOrder`:
   - Drop all `decimal.NewFromString` and side-switch logic (moves into decrypter).
   - Validate: `len(commitment) > 0`, `validateProofFormat(proof)`, `len(ciphertext) > 0`, `len(ciphertext) <= MaxCiphertextBytes`.
   - Call `s.engine.PlaceEncryptedOrder(ctx, req.Commitment, req.Proof, req.EncryptedPayload)`.
   - Map `utils.ErrCommitmentMismatch` → `codes.InvalidArgument` with `MsgCommitmentMismatch`.

5. Rewrite `handler_test.go` fixtures:
   - Helper `buildReq(t, DecryptedOrder)` → JSON-encodes the decrypted order, computes stub commitment = sha256, returns a `PlaceOrderRequest`. One helper; reuse across tests.
   - Update `placeTestOrder`, `TestPlaceOrder_Success`, `TestPlaceOrder_ValidationErrors`.
   - **New tests (mandatory)**:
     - `TestPlaceOrder_MissingCommitment` — empty commitment → `InvalidArgument`.
     - `TestPlaceOrder_MissingCiphertext` — empty ciphertext → `InvalidArgument`.
     - `TestPlaceOrder_CiphertextTooLarge` — `> MaxCiphertextBytes` → `InvalidArgument`, `MsgCiphertextTooLarge`.
     - **`TestPlaceOrder_CommitmentMismatch` — canary test for the entire spec alignment**. Build a request whose commitment hashes one order but ciphertext decrypts to a different one. Expect `InvalidArgument` with `MsgCommitmentMismatch`. **This test is the contract. If it passes, the privacy model holds. If it does not exist, this phase is not done.**

6. **Gate**: `go build ./... && go test ./...` — both green. Commit as "Phase 2: encrypt-only wire protocol".

---

## Phase 3 — Event store hygiene

**Files**
- `server/engine/event/event.go`
- `server/engine/core/engine.go`
- `server/engine/core/engine_test.go`
- `server/engine/core/batch_test.go`
- `README.md`

**Steps**

1. Rewrite `OrderPlaced` payload:

   ```go
   type OrderPlaced struct {
       OrderID      uuid.UUID
       Commitment   []byte
       Proof        []byte
       Ciphertext   []byte
       SubmittedAt  time.Time
       ExpiresAt    time.Time
   }
   ```

   Plaintext `model.Order` no longer hits disk.

2. `engine.PlaceEncryptedOrder`:
   - Generate `OrderID` up front.
   - Persist `OrderPlaced` with `{commitment, proof, ciphertext, …}`.
   - Keep decrypted `model.Order` in memory only (orderbook, pendingBatches, etc. already in-RAM).

3. `engine.Recover()`:
   - Replay `OrderPlaced` events. For each, call `e.decrypter.Decrypt(ciphertext)` to rebuild the in-memory `model.Order`. Commitment re-validated on replay (defends against tampered event log).
   - Recovery cost now scales with decrypt-per-pending-order. Acceptable — document it.

4. Update `batch_test.go` recovery tests: fixtures now build encrypted payloads instead of calling `PlaceOrder` with plaintext. Reuse the `buildReq` helper pattern from Phase 2, or equivalent in the core test package.

5. **Canary test** in `engine_test.go` or a new `privacy_test.go`:

   ```go
   func TestEventStoreContainsNoPlaintext(t *testing.T) {
       // Submit an order via PlaceEncryptedOrder with price "1234.5678".
       // Read every event from the store, serialize to JSON/gob, and assert
       // the bytes "1234.5678" never appear. This is the dark-pool invariant.
   }
   ```

   This test is the canary for Phase 3. If it fails, plaintext is leaking into persistence.

6. Update `README.md` Architecture section:
   - Explicit line: "Plaintext orders exist only in engine RAM during the auction window. The event log contains ciphertext + commitment + proof only."
   - Update the mermaid diagram's "Decrypt order" node to reference `Decrypter` seam.

7. **Gate**: `go build ./... && go test ./...` — both green. All three phase canary tests (`TestPlaceEncryptedOrder_CommitmentMismatch`, `TestPlaceOrder_CommitmentMismatch`, `TestEventStoreContainsNoPlaintext`) must pass. Commit as "Phase 3: plaintext out of event log".

---

## Master checklist

- [ ] Phase 1: `Decrypter` + `NoopDecrypter` + `PlaceEncryptedOrder` + commitment check. Existing `PlaceOrder` untouched. Tests green.
- [ ] Phase 2: proto rewritten, regen committed separately, handler rewired. `TestPlaceOrder_CommitmentMismatch` exists and passes.
- [ ] Phase 3: `OrderPlaced` is ciphertext-only, recover path decrypts, `TestEventStoreContainsNoPlaintext` passes. README updated.
- [ ] `go build ./...` + `go test ./...` green after each phase.
- [ ] No real crypto introduced — all stubs carry `TODO(zk-pipeline)` tags linking to tracked issues.
- [ ] Deprecated plaintext `Engine.PlaceOrder` signature: decide in Phase 3 whether to remove entirely or keep for internal engine-test helpers. If removed, grep the repo for every caller first.

## Non-goals (out of scope for this handoff)

- Real Pedersen commitment scheme — stub is `sha256`, replace when circuit lands.
- Real ECIES / AES-GCM decrypt — stub is JSON passthrough.
- halo2 / arkworks proof verification — still format-only validation at API boundary.
- Operator keypair management / pubkey distribution to clients.
- Rust aggregator CLI integration.
- Frontend changes.

## Gotchas / prior review notes

- Aggregator currently runs under `e.mu` (`engine.go` around the `Aggregate` call site). This plan does not fix that, but keep it in mind — decrypt under `e.mu` has the same property. Acceptable for scaffolding; move to async alongside the Rust CLI integration.
- `server/api/proto/buf.gen.yaml` is currently **untracked** (appears in `git status` as `??`). First step of Phase 2 is to confirm it's the right config and commit it.
- `TestBatchLifecycle_ProofPersistedAndReusedOnResubmit` must still pass after Phase 3. The aggregated proof on `BatchSubmitted` is orthogonal to per-order ciphertext — don't conflate them.
