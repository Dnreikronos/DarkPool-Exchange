# I2.10 — End-to-end real order placement (#99)

**Status:** design approved, pre-implementation
**Worktree:** `darkpool-wt/99-order-e2e` · branch `feat/issue-99-order-e2e`
**File scope:** `front/app/app/trade/_{components,hooks,lib}/entry/` only, plus one
approved additive edit to `front/lib/sdk/client.ts` (see §7).

## Why

F1.9 shipped the order-entry panel with a *mock* multi-stage submit: a pure
orchestrator (`runSubmission`) walks `preparing → proving → encrypting →
submitting`, but every stage is a fixed `setTimeout` and the terminal action
just pushes to the in-memory mock store. #99 replaces those internals with the
real pipeline — **build witness → WASM prove → ECIES encrypt → POST
/v1/orders** — without changing the F1.9 layout. The mock path stays intact
behind the existing mock gate.

## Decisions (locked in brainstorming)

1. **Mock vs real:** runtime gate. When the `placeOrder` RPC resolves to a mock
   (`config.useMocks` or `NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER`), keep today's
   behaviour exactly (mock store, no worker, no fetch, fixed delays). Otherwise
   run the real pipeline. F1.9 stays demoable; node-only tests stay green.
2. **Error UX:** dedicated **inline** area below the `PlaceButton` (not the
   toast). Specific message per tonic code; on 429 surface `retry-after`; on 5xx
   a collapsible `<details>` with `x-request-id`. Success keeps the toast.
3. **Progress:** drop fixed-duration bar weighting. Each running label shows
   **real elapsed seconds** for the current stage (e.g. `GENERATING PROOF ·
   4.2s`); the proving bar follows `useProver().progress.pct`; the fast stages
   fill on completion.
4. **Architecture:** generalize the single pure orchestrator to take an ordered
   list of injectable async stage steps. Both mock and real assemble steps;
   one state machine, still node-testable.

## Backend facts established (read-only investigation)

- **Commitment binding.** The operator *ignores* the client commitment and
  recomputes a canonical Poseidon commitment from the decrypted order +
  `commitment_key` + per-boot nonce (`dp-engine/src/engine.rs:487-490`,
  `:824-829`). So the client never sends a salt.
- **Decrypted payload shape** (`dp-crypto/src/decrypted_order.rs:8`):
  `{trader, pair, side, price, size, commitment_key, ttl}` — matches TS
  `serializeOrder` (`front/lib/crypto/order-payload.ts`) byte-for-byte. **No
  salt field; do not add one.**
- **Witness shape** (`dp-zk-wasm/src/lib.rs:9-16`):
  `{commitment_key, side:u8, price, size, salt_hex}`. `commitment_key` must be
  **valid hex** (`hex::decode` at `:22`); `salt_hex` must decode to 32 bytes.
  `trader_id` is derived from `commitment_key`, not from the wallet address.
- **`prove_order_wasm` return:** `[proofLen u32 LE | vkLen u32 LE |
  commitmentLen u32 LE | proof | vk | commitment]` — already parsed by
  `front/lib/prover/prover.worker.ts`.

### Known limitation (surface in PR, not fixed here)

The per-order WASM proof is a **placeholder** (`dp-client-v0`): the WASM circuit
derives `trader_id` from `commitment_key` while the operator derives it from the
real trader address, with different salts, so the proof's commitment would not
match an operator *content* check. Because the operator does **not** content-
verify the client commitment today, a real order is still accepted. #99 builds
the correctly-*shaped* pipeline the operator accepts; binding the proof to the
real trader identity is a backend/circuit follow-up. This is called out in the
PR body, not solved in #99 (prover/circuit are out of scope).

## Architecture

### Stage-step orchestrator (`_hooks/entry/useSubmitStages.ts`)

Generalize `runSubmission` to walk an ordered list of steps instead of
hard-coding stage behaviour:

```ts
interface StageStep<Ctx> {
  id: SubmitStageId
  run: (ctx: Ctx) => Promise<Ctx>   // pure-ish; returns next ctx
}
```

The orchestrator, per step: check abort → emit `{kind:'running', stage,
stageStartedAtMs: now()}` → `await step.run(ctx)` → emit running boundary →
next. After the last step: `success`, hold, `idle`. `catch` → `error`. A `now:
() => number` clock is injected (default `performance.now()`/`Date.now()`),
keeping the existing manually-pumped-scheduler test style deterministic.

`SubmissionPhase` running variant gains **optional** fields so
`OrderEntry.stories.tsx` / `PlaceButtonStages` keep compiling:

```ts
| { kind: 'running'; stage: SubmitStageId; progress: number
    stageStartedAtMs?: number; provingPct?: number }
```

`progress` is retained for the quick-stage bar fill; the proving bar reads
`provingPct` when present.

### Mock steps (unchanged behaviour)

Built when the gate says mock: each step is `delay(STAGE_DURATIONS_MS[id])`, and
the `submitting` step calls the injected mock `placeOrder`. Net effect identical
to today, so the F1.9 demo and the existing `useSubmitStages.test.ts` assertions
hold (with timing now via the injected clock/delay).

### Real steps (`_hooks/entry/useRealSubmission.ts` — new)

A hook that assembles the four real steps from injected dependencies and returns
them for the orchestrator. Pure step *logic* lives in `_lib/entry/` so it is
node-testable; the hook only wires React-sourced deps.

- **preparing:** generate `commitmentKey = randomHex(32)` and `saltHex =
  randomHex(32)` (one each, this submission). Build the witness
  `{commitment_key, side, price, size, salt_hex}` and the `DecryptedOrderPayload`
  `{trader, pair, side, price, size, commitment_key, ttl}` — **same
  `commitment_key` in both.** `trader` from `useTraderId()`; `pair` from policy
  (`ETH/USDC` constant); `side` mapped buy→0/sell→1; `ttl` a policy constant.
- **proving:** `await prove(witness)` (from `useProver`) → `{proof,
  commitment}`. Prover progress is surfaced into `provingPct` (see §progress).
- **encrypting:** `serializeOrder(payload)` → `encryptOrder(bytes,
  operatorPubkey)` (from `useOperatorPubkey`) → `encryptedPayload`.
- **submitting:** `client.placeOrder(create(PlaceOrderRequestSchema,
  {commitment, proof, encryptedPayload}))` (from `useDarkPoolClient`).

Numeric fields stay decimal strings end-to-end (price/size via the form;
`parseUnits` not needed — wire is strings). Bytes cross as `Uint8Array` into the
proto `create()`, which the SDK base64-encodes.

### Wiring (`_components/entry/OrderEntry.tsx`)

`OrderEntry` decides mock vs real once and passes the right step list +
`onError` into `useSubmitStages`. Real deps (`useOperatorPubkey`, `useProver`,
`useDarkPoolClient`, `useTraderId`) are read unconditionally at the top (hooks
rules); they're inert in mock mode. The injectable `placeOrder`/`delay` props
remain for Storybook/tests.

## Progress & elapsed (`ProveSubmitStages.tsx`, `policy.ts`)

- A small `useElapsed(stageStartedAtMs, running)` ticker (interval ~100ms)
  computes live seconds for the current stage; label becomes
  `${STAGE_LABELS[stage]} · ${secs}s …`.
- Proving bar width = `provingPct/100`; other stages fill to their boundary on
  completion (existing `progress`).
- `STAGE_DURATIONS_MS` is retained **only** for the mock path's delays; its role
  as the real progress driver is removed. `progressAtStartOfStage` /
  `progressAtEndOfStage` stay for mock/quick-stage fill.

## Error handling (`_lib/entry/submit-error.ts` — new, + inline UI)

- New pure mapper `submitErrorMessage(err: DarkPoolError): string` covering
  **every** `DarkPoolErrorName` (the 17 tonic codes), tone per DESIGN. E.g.
  `RESOURCE_EXHAUSTED` → rate-limit copy, `UNAVAILABLE`/network → "service
  unreachable, retry", `UNAUTHENTICATED`/`PERMISSION_DENIED` → auth copy,
  `INVALID_ARGUMENT`/`FAILED_PRECONDITION` → "order rejected" + server message,
  `INTERNAL`/unknown → generic server-error copy.
- A new `SubmitError` view (in `ProveSubmitStages.tsx` or a sibling
  `SubmitError.tsx`) renders below the button when `phase.kind === 'error'`:
  the mapped message; if `retryAfter` present (429) a "retry in Ns" line; if
  `httpStatus >= 500` a collapsible `<details>` showing `x-request-id` and the
  raw server message. `role="alert"`, persists until the next submit.
- The error phase must carry the structured error, not just a string. Extend the
  error phase to `{kind:'error'; message: string; detail?: {code, codeName,
  httpStatus, requestId?, retryAfter?}}` (optional → stories unaffected). The
  orchestrator's `catch` populates `detail` when the thrown value is a
  `DarkPoolError`.

## §7 — SDK header capture (approved scope-crossing)

`DarkPoolError` currently drops response headers. Additive change to
`front/lib/sdk/client.ts`:

- Add optional `requestId?: string | null` and `retryAfter?: string | null` to
  `DarkPoolError` (constructor opts), defaulting null.
- In `parseErrorResponse`, read `response.headers.get('x-request-id')` and
  (`retry-after`) and pass them through.

No behaviour change for existing callers (new fields optional). Noted as a
scope-crossing in the PR.

## Testing (TDD, node-only vitest — no DOM)

- **Orchestrator** (`useSubmitStages.test.ts`, extend): step list drives phases
  in order; abort/stale still drops runs; injected clock makes elapsed
  deterministic; error phase carries `detail` when a `DarkPoolError` is thrown;
  success/idle unchanged.
- **Real steps** (`_lib/entry` unit tests): witness + payload share one
  `commitment_key`; `commitment_key`/`salt_hex` are valid hex of the right
  length; side mapping; payload shape equals `DecryptedOrderPayload`; `placeOrder`
  request carries the prove output bytes.
- **Error mapper:** a case per `DarkPoolErrorName`; 429 surfaces retryAfter; 5xx
  surfaces requestId.
- **SDK** (`lib/sdk/client.test.ts`, extend): `parseErrorResponse` captures
  `x-request-id` and `retry-after`.
- **Composition** (`OrderEntry.test.tsx`, extend): mock gate → no
  prover/fetch; render-to-static smoke for the inline error block.

## Out of scope / non-goals

- The ZK circuit trader-binding limitation (backend follow-up).
- `lib/prover`, `lib/crypto`, proto, mock store — consumed read-only.
- Multi-pair, Shell wiring, streaming.

## Files

- Edit: `_hooks/entry/useSubmitStages.ts`, `_components/entry/OrderEntry.tsx`,
  `_components/entry/ProveSubmitStages.tsx`, `_lib/entry/policy.ts`,
  `_components/entry/index.ts` / `_lib/entry/index.ts` (exports),
  `front/lib/sdk/client.ts` (§7).
- New: `_hooks/entry/useRealSubmission.ts`, `_lib/entry/build-submission.ts`
  (pure witness/payload builders), `_lib/entry/submit-error.ts`, and their tests.
- Extend tests: `useSubmitStages.test.ts`, `OrderEntry.test.tsx`,
  `lib/sdk/client.test.ts`.
