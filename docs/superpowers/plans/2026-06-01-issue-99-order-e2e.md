# I2.10 End-to-end Real Order Placement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the F1.9 mock order-submit internals with the real pipeline — build witness → WASM prove → ECIES encrypt → POST /v1/orders — behind the existing mock gate, with real per-stage elapsed timing and a full tonic-error → inline-UI mapping.

**Architecture:** One pure async orchestrator (`runSubmission`) walks an injected ordered list of `StageStep`s. The mock path and the real path each assemble that list from injected dependencies, so a single node-testable state machine drives both. Pure builders (witness/payload/error-mapping) live in `_lib/entry/`; React hooks supply the real dependencies.

**Tech Stack:** Next.js 14 / React 18 / TypeScript, Vitest (node-only, no DOM), `@bufbuild/protobuf`, `decimal.js`, the `lib/prover` (Web Worker + WASM) and `lib/crypto` (ECIES) modules consumed read-only.

**Spec:** `docs/superpowers/specs/2026-05-31-issue-99-order-e2e-design.md`

**Conventions for every task:**
- All commands run from the worktree front dir. Each Bash invocation starts fresh, so prefix with the cd: `cd /home/mario/darkpool-wt/99-order-e2e/front && <cmd>`.
- Test a single file: `npx vitest run <relative-path>`.
- Commits: author is the human user only — **never** add a Claude co-author/footer trailer.

---

## File Structure

| File | Responsibility | Task |
|------|----------------|------|
| `lib/sdk/client.ts` (modify) | `DarkPoolError` captures `x-request-id` + `retry-after` | 1 |
| `lib/sdk/client.test.ts` (modify) | header-capture tests | 1 |
| `app/app/trade/_lib/entry/policy.ts` (modify) | add `ORDER_PAIR`, `ORDER_TTL_NS` | 2 |
| `app/app/trade/_lib/entry/submit-error.ts` (new) | tonic-code → user message + structured detail | 3 |
| `app/app/trade/_lib/entry/submit-error.test.ts` (new) | mapper tests | 3 |
| `app/app/trade/_lib/entry/build-submission.ts` (new) | pure `randomHex`/`buildWitness`/`buildOrderPayload`/`createRealSteps` | 4 |
| `app/app/trade/_lib/entry/build-submission.test.ts` (new) | builder + step tests | 4 |
| `app/app/trade/_hooks/entry/useSubmitStages.ts` (modify) | step-list orchestrator + `buildMockSteps` | 5 |
| `app/app/trade/_hooks/entry/useSubmitStages.test.ts` (modify) | rewritten to the new API | 5 |
| `app/app/trade/_hooks/entry/useRealSubmission.ts` (new) | assemble real steps from React hooks | 6 |
| `app/app/trade/_components/entry/ProveSubmitStages.tsx` (modify) | elapsed ticker, `provingPct` bar, `SubmitError` view | 7 |
| `app/app/trade/_components/entry/OrderEntry.tsx` (modify) | mock-vs-real gate, wiring, inline error | 8 |
| `app/app/trade/_components/entry/OrderEntry.test.tsx` (modify) | gate + inline-error smoke | 8 |
| `_lib/entry/index.ts`, `_hooks/entry`/`_components/entry/index.ts` (modify) | exports | 9 |

---

## Task 1: SDK captures error headers (approved scope-crossing)

**Files:**
- Modify: `lib/sdk/client.ts` (`DarkPoolError` class ~81-102; `parseErrorResponse` ~292-306)
- Test: `lib/sdk/client.test.ts` (append a describe block)

- [ ] **Step 1: Write the failing tests** — append to `lib/sdk/client.test.ts`, just before the final `beforeEach(...)` block:

```ts
// ─── error header capture (#99 / C7) ──────────────────────────────────────

describe('RestClient error header capture', () => {
  it('captures retry-after on a 429 response', async () => {
    const { fetch } = captureFetch(
      new Response(JSON.stringify({ code: DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED, message: 'slow down' }), {
        status: 429,
        headers: { 'content-type': 'application/json', 'retry-after': '30', 'x-request-id': 'req-429' },
      })
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      client.getOrder(create(GetOrderRequestSchema, { orderId: 'x' }))
    ).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED,
      retryAfter: '30',
      requestId: 'req-429',
    })
  })

  it('captures x-request-id on a 500 response', async () => {
    const { fetch } = captureFetch(
      new Response('', { status: 500, headers: { 'x-request-id': 'req-500' } })
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      client.getOrder(create(GetOrderRequestSchema, { orderId: 'x' }))
    ).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.INTERNAL,
      requestId: 'req-500',
      retryAfter: null,
    })
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run lib/sdk/client.test.ts`
Expected: FAIL — the two new cases fail (`retryAfter`/`requestId` are `undefined`, not `'30'`/`null`).

- [ ] **Step 3: Add the fields to `DarkPoolError`** — replace the class fields + constructor (the block currently starting `export class DarkPoolError extends Error {` through the closing brace of the constructor) with:

```ts
export class DarkPoolError extends Error {
  readonly code: DarkPoolErrorCode
  readonly codeName: DarkPoolErrorName
  readonly httpStatus: number | null
  readonly retryable: boolean
  /** `x-request-id` response header, when the server sent one (C7). */
  readonly requestId: string | null
  /** `retry-after` response header (seconds or HTTP-date), when present. */
  readonly retryAfter: string | null

  constructor(
    code: DarkPoolErrorCode,
    message: string,
    opts: {
      httpStatus?: number | null
      requestId?: string | null
      retryAfter?: string | null
      cause?: unknown
    } = {}
  ) {
    super(message)
    this.name = 'DarkPoolError'
    this.code = code
    this.codeName = CODE_NAMES.get(code) ?? 'UNKNOWN'
    this.httpStatus = opts.httpStatus ?? null
    this.requestId = opts.requestId ?? null
    this.retryAfter = opts.retryAfter ?? null
    this.retryable = isRetryableCode(code)
    if (opts.cause !== undefined) {
      ;(this as { cause?: unknown }).cause = opts.cause
    }
  }
}
```

- [ ] **Step 4: Read headers in `parseErrorResponse`** — replace the `return new DarkPoolError(...)` at the end of `parseErrorResponse` with:

```ts
  return new DarkPoolError(code, message, {
    httpStatus: response.status,
    requestId: response.headers.get('x-request-id'),
    retryAfter: response.headers.get('retry-after'),
  })
```

- [ ] **Step 5: Run, verify pass**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run lib/sdk/client.test.ts`
Expected: PASS (all, including the two new cases).

- [ ] **Step 6: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/lib/sdk/client.ts front/lib/sdk/client.test.ts && \
git commit -m "feat(#99): capture x-request-id and retry-after in DarkPoolError"
```

---

## Task 2: Policy constants for the real order

**Files:**
- Modify: `app/app/trade/_lib/entry/policy.ts` (append)

No test (constants). They are exercised in Tasks 4 & 6.

- [ ] **Step 1: Append the constants** to `policy.ts`:

```ts
/**
 * Canonical pair string the operator's registry accepts. The engine
 * canonicalises to uppercase + slash (dp-types `Pair::parse`), and the
 * single seeded market is "ETH/USDC". Multi-pair is gated on #29.
 */
export const ORDER_PAIR = 'ETH/USDC'

/**
 * Order time-to-live, in NANOSECONDS. The engine reads `ttl` as a duration
 * in nanoseconds and sets `expires_at = now + ttl`
 * (dp-engine/src/engine.rs:551,557). 5 minutes keeps the order alive across
 * a few auction rounds without lingering.
 */
export const ORDER_TTL_NS = 5 * 60 * 1_000_000_000 // 300_000_000_000
```

- [ ] **Step 2: Typecheck**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx tsc --noEmit`
Expected: PASS (no errors).

- [ ] **Step 3: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_lib/entry/policy.ts && \
git commit -m "feat(#99): add ORDER_PAIR and ORDER_TTL_NS policy constants"
```

---

## Task 3: Error mapper (`submit-error.ts`)

**Files:**
- Create: `app/app/trade/_lib/entry/submit-error.ts`
- Test: `app/app/trade/_lib/entry/submit-error.test.ts`

- [ ] **Step 1: Write the failing test** — `submit-error.test.ts`:

```ts
import { describe, expect, it } from 'vitest'

import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/api-client'

import { mapSubmissionError, submitErrorMessage, toSubmitErrorDetail } from './submit-error'

describe('submitErrorMessage', () => {
  const cases: Array<[keyof typeof DARK_POOL_ERROR_CODES, RegExp]> = [
    ['INVALID_ARGUMENT', /rejected/i],
    ['FAILED_PRECONDITION', /rejected/i],
    ['OUT_OF_RANGE', /rejected/i],
    ['UNAUTHENTICATED', /sign in|authenticate|api key/i],
    ['PERMISSION_DENIED', /not allowed|permission/i],
    ['NOT_FOUND', /not found/i],
    ['ALREADY_EXISTS', /already/i],
    ['RESOURCE_EXHAUSTED', /too many|rate/i],
    ['UNAVAILABLE', /unavailable|unreachable|retry/i],
    ['DEADLINE_EXCEEDED', /timed out|timeout/i],
    ['ABORTED', /try again|conflict/i],
    ['UNIMPLEMENTED', /not (yet )?available|unsupported/i],
    ['INTERNAL', /server error|something went wrong/i],
    ['UNKNOWN', /server error|something went wrong/i],
    ['DATA_LOSS', /server error|something went wrong/i],
    ['CANCELLED', /cancell?ed/i],
    ['OK', /server error|something went wrong/i],
  ]
  it.each(cases)('maps %s to a specific message', (name, re) => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES[name], 'raw server message')
    expect(submitErrorMessage(err)).toMatch(re)
  })
})

describe('toSubmitErrorDetail', () => {
  it('carries code/codeName/httpStatus/requestId/retryAfter', () => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED, 'slow down', {
      httpStatus: 429,
      requestId: 'req-1',
      retryAfter: '30',
    })
    expect(toSubmitErrorDetail(err)).toEqual({
      code: DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED,
      codeName: 'RESOURCE_EXHAUSTED',
      httpStatus: 429,
      requestId: 'req-1',
      retryAfter: '30',
      serverMessage: 'slow down',
    })
  })
})

describe('mapSubmissionError', () => {
  it('returns message + detail for a DarkPoolError', () => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES.INTERNAL, 'boom', {
      httpStatus: 500,
      requestId: 'req-x',
    })
    const out = mapSubmissionError(err)
    expect(out.message).toMatch(/server error|something went wrong/i)
    expect(out.detail?.requestId).toBe('req-x')
  })

  it('passes through a plain Error message with no detail', () => {
    expect(mapSubmissionError(new Error('worker died'))).toEqual({ message: 'worker died' })
  })

  it('stringifies non-Error throwables', () => {
    expect(mapSubmissionError('nope')).toEqual({ message: 'nope' })
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_lib/entry/submit-error.test.ts`
Expected: FAIL — module `./submit-error` not found.

- [ ] **Step 3: Implement** — `submit-error.ts`:

```ts
// Maps a DarkPoolError (mirrors tonic::Code; see crates/dp-api/src/rest.rs
// ApiError::into_response) to user-facing copy for the order-entry inline
// error area, plus a structured detail the UI uses for the 429 retry hint
// and the 5xx collapsible technical block (x-request-id). Tone follows
// DESIGN-INSPIRATIONS: informative, no apology, no exclamation.

import { DarkPoolError, type DarkPoolErrorName } from '@/lib/api-client'

export interface SubmitErrorDetail {
  code: number
  codeName: DarkPoolErrorName
  httpStatus: number | null
  requestId: string | null
  retryAfter: string | null
  serverMessage: string
}

const MESSAGES: Record<DarkPoolErrorName, string> = {
  OK: 'Server error. Try again.',
  CANCELLED: 'Order cancelled before it was accepted.',
  UNKNOWN: 'Server error. Try again.',
  INVALID_ARGUMENT: 'Order rejected: the engine refused these values.',
  DEADLINE_EXCEEDED: 'The request timed out. Try again.',
  NOT_FOUND: 'Market not found.',
  ALREADY_EXISTS: 'This order was already submitted.',
  PERMISSION_DENIED: 'Not allowed: this key cannot place orders.',
  RESOURCE_EXHAUSTED: 'Too many requests. Slow down and retry.',
  FAILED_PRECONDITION: 'Order rejected: the market is not accepting it right now.',
  ABORTED: 'The order conflicted with a concurrent change. Try again.',
  OUT_OF_RANGE: 'Order rejected: a value is out of the accepted range.',
  UNIMPLEMENTED: 'This action is not available yet.',
  INTERNAL: 'Something went wrong on the server. Try again.',
  UNAVAILABLE: 'The engine is unreachable. Retry in a moment.',
  DATA_LOSS: 'Something went wrong on the server. Try again.',
  UNAUTHENTICATED: 'Authentication failed: check the API key.',
}

export function submitErrorMessage(err: DarkPoolError): string {
  return MESSAGES[err.codeName] ?? MESSAGES.UNKNOWN
}

export function toSubmitErrorDetail(err: DarkPoolError): SubmitErrorDetail {
  return {
    code: err.code,
    codeName: err.codeName,
    httpStatus: err.httpStatus,
    requestId: err.requestId,
    retryAfter: err.retryAfter,
    serverMessage: err.message,
  }
}

export function mapSubmissionError(err: unknown): { message: string; detail?: SubmitErrorDetail } {
  if (err instanceof DarkPoolError) {
    return { message: submitErrorMessage(err), detail: toSubmitErrorDetail(err) }
  }
  if (err instanceof Error) return { message: err.message }
  return { message: String(err) }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_lib/entry/submit-error.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_lib/entry/submit-error.ts front/app/app/trade/_lib/entry/submit-error.test.ts && \
git commit -m "feat(#99): map tonic error codes to inline order-entry messages"
```

---

## Task 4: Pure submission builders (`build-submission.ts`)

**Files:**
- Create: `app/app/trade/_lib/entry/build-submission.ts`
- Test: `app/app/trade/_lib/entry/build-submission.test.ts`

This file defines the `StageStep` shape, the witness/payload builders, and a
pure `createRealSteps` factory driven by injected dependencies (so it runs in
node without a worker/network).

- [ ] **Step 1: Write the failing test** — `build-submission.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest'

import type { DecryptedOrderPayload } from '@/lib/crypto'

import { buildOrderPayload, buildWitness, createRealSteps, randomHex } from './build-submission'
import { ORDER_PAIR, ORDER_TTL_NS } from './policy'

const TRADER = '0x1234567890123456789012345678901234567890'

describe('randomHex', () => {
  it('returns 2*n lowercase hex chars', () => {
    const hex = randomHex(32)
    expect(hex).toMatch(/^[0-9a-f]{64}$/)
  })
  it('returns a different value each call', () => {
    expect(randomHex(32)).not.toBe(randomHex(32))
  })
})

describe('buildWitness', () => {
  it('maps buy→0 and carries the hex key + salt and string price/size', () => {
    const w = buildWitness({
      commitmentKey: 'aa'.repeat(32),
      saltHex: 'bb'.repeat(32),
      side: 'buy',
      price: '3000.5',
      size: '0.25',
    })
    expect(w).toEqual({
      commitment_key: 'aa'.repeat(32),
      side: 0,
      price: '3000.5',
      size: '0.25',
      salt_hex: 'bb'.repeat(32),
    })
  })
  it('maps sell→1', () => {
    expect(buildWitness({ commitmentKey: 'aa', saltHex: 'bb', side: 'sell', price: '1', size: '1' }).side).toBe(1)
  })
})

describe('buildOrderPayload', () => {
  it('produces the DecryptedOrder shape with side as 0|1 and ttl in ns', () => {
    const p: DecryptedOrderPayload = buildOrderPayload({
      trader: TRADER,
      pair: ORDER_PAIR,
      side: 'sell',
      price: '3000.5',
      size: '0.25',
      commitmentKey: 'aa'.repeat(32),
      ttlNs: ORDER_TTL_NS,
    })
    expect(p).toEqual({
      trader: TRADER,
      pair: 'ETH/USDC',
      side: 1,
      price: '3000.5',
      size: '0.25',
      commitment_key: 'aa'.repeat(32),
      ttl: 300_000_000_000,
    })
  })
})

describe('createRealSteps', () => {
  function deps(overrides = {}) {
    return {
      trader: TRADER,
      pair: ORDER_PAIR,
      ttlNs: ORDER_TTL_NS,
      side: 'buy' as const,
      price: '3000',
      size: '0.5',
      randomHex: vi
        .fn<[number], string>()
        .mockReturnValueOnce('cc'.repeat(32)) // commitment_key
        .mockReturnValueOnce('dd'.repeat(32)), // salt_hex
      getOperatorPubkey: vi.fn(() => new Uint8Array([0x04, 0x01])),
      prove: vi.fn(async () => ({ proof: new Uint8Array([1]), commitment: new Uint8Array([2]) })),
      serialize: vi.fn(() => new Uint8Array([9, 9])),
      encrypt: vi.fn(() => new Uint8Array([7, 7])),
      placeOrder: vi.fn(async () => undefined),
      ...overrides,
    }
  }

  const ctx = { aborted: () => false }

  it('emits four steps in pipeline order', () => {
    const steps = createRealSteps(deps())
    expect(steps.map((s) => s.id)).toEqual(['preparing', 'proving', 'encrypting', 'submitting'])
  })

  it('threads ONE commitment_key into both witness and payload', async () => {
    const d = deps()
    const steps = createRealSteps(d)
    await steps[0].run(ctx) // preparing
    await steps[1].run(ctx) // proving
    await steps[2].run(ctx) // encrypting

    // witness passed to prove
    expect(d.prove).toHaveBeenCalledWith(
      expect.objectContaining({ commitment_key: 'cc'.repeat(32), salt_hex: 'dd'.repeat(32), side: 0 })
    )
    // payload passed to serialize has the SAME commitment_key, no salt field
    const payload = d.serialize.mock.calls[0][0]
    expect(payload.commitment_key).toBe('cc'.repeat(32))
    expect(payload).not.toHaveProperty('salt_hex')
    expect(payload.trader).toBe(TRADER)
  })

  it('submits the prove output bytes through placeOrder', async () => {
    const d = deps()
    const steps = createRealSteps(d)
    for (const s of steps) await s.run(ctx)
    expect(d.encrypt).toHaveBeenCalledWith(new Uint8Array([9, 9]), new Uint8Array([0x04, 0x01]))
    expect(d.placeOrder).toHaveBeenCalledWith({
      commitment: new Uint8Array([2]),
      proof: new Uint8Array([1]),
      encryptedPayload: new Uint8Array([7, 7]),
    })
  })

  it('throws a clear error when the trader is missing', async () => {
    const steps = createRealSteps(deps({ trader: '' }))
    await expect(steps[0].run(ctx)).rejects.toThrow(/wallet/i)
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_lib/entry/build-submission.test.ts`
Expected: FAIL — module `./build-submission` not found.

- [ ] **Step 3: Implement** — `build-submission.ts`:

```ts
// Pure builders for the real submission pipeline (#99). No React, no
// network, no worker — everything is injected so this is node-testable.
// The order's blinding nonce is `commitment_key`: ONE random hex value is
// generated per submission and threaded into BOTH the prover witness and
// the encrypted order payload. The operator recomputes the canonical
// Poseidon commitment from the decrypted payload, so the client never
// sends a salt (dp-engine/src/engine.rs:487-490).

import type { DecryptedOrderPayload } from '@/lib/crypto'
import type { WitnessInput } from '@/lib/prover'

import type { OrderSide } from './validate'

/** A single stage of the submission state machine. */
export interface StageStep {
  id: 'preparing' | 'proving' | 'encrypting' | 'submitting'
  run: (ctx: { aborted: () => boolean }) => Promise<void>
}

function sideToNum(side: OrderSide): 0 | 1 {
  return side === 'buy' ? 0 : 1
}

/** Cryptographically-random lowercase hex string of `nBytes` bytes. */
export function randomHex(nBytes: number): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(nBytes)), (b) =>
    b.toString(16).padStart(2, '0')
  ).join('')
}

export function buildWitness(args: {
  commitmentKey: string
  saltHex: string
  side: OrderSide
  price: string
  size: string
}): WitnessInput {
  return {
    commitment_key: args.commitmentKey,
    side: sideToNum(args.side),
    price: args.price,
    size: args.size,
    salt_hex: args.saltHex,
  }
}

export function buildOrderPayload(args: {
  trader: string
  pair: string
  side: OrderSide
  price: string
  size: string
  commitmentKey: string
  ttlNs: number
}): DecryptedOrderPayload {
  return {
    trader: args.trader,
    pair: args.pair,
    side: sideToNum(args.side),
    price: args.price,
    size: args.size,
    commitment_key: args.commitmentKey,
    ttl: args.ttlNs,
  }
}

export interface RealStepDeps {
  trader: string
  pair: string
  ttlNs: number
  side: OrderSide
  price: string
  size: string
  randomHex: (nBytes: number) => string
  /** Latest resolved operator SEC1 pubkey bytes; throws if not loaded yet. */
  getOperatorPubkey: () => Uint8Array
  prove: (witness: WitnessInput) => Promise<{ proof: Uint8Array; commitment: Uint8Array }>
  serialize: (payload: DecryptedOrderPayload) => Uint8Array
  encrypt: (bytes: Uint8Array, pubkey: Uint8Array) => Uint8Array
  placeOrder: (req: {
    commitment: Uint8Array
    proof: Uint8Array
    encryptedPayload: Uint8Array
  }) => Promise<unknown>
}

/**
 * Build the four real stage steps. The steps share a private draft so the
 * commitment_key minted in `preparing` reaches `proving`/`encrypting`, and
 * the prove output reaches `submitting`.
 */
export function createRealSteps(deps: RealStepDeps): StageStep[] {
  const draft: {
    witness?: WitnessInput
    payload?: DecryptedOrderPayload
    proof?: Uint8Array
    commitment?: Uint8Array
    encryptedPayload?: Uint8Array
  } = {}

  return [
    {
      id: 'preparing',
      run: async () => {
        if (!deps.trader) throw new Error('Connect a wallet to place orders.')
        const commitmentKey = deps.randomHex(32)
        const saltHex = deps.randomHex(32)
        draft.witness = buildWitness({
          commitmentKey,
          saltHex,
          side: deps.side,
          price: deps.price,
          size: deps.size,
        })
        draft.payload = buildOrderPayload({
          trader: deps.trader,
          pair: deps.pair,
          side: deps.side,
          price: deps.price,
          size: deps.size,
          commitmentKey,
          ttlNs: deps.ttlNs,
        })
      },
    },
    {
      id: 'proving',
      run: async () => {
        const { proof, commitment } = await deps.prove(draft.witness!)
        draft.proof = proof
        draft.commitment = commitment
      },
    },
    {
      id: 'encrypting',
      run: async () => {
        const bytes = deps.serialize(draft.payload!)
        draft.encryptedPayload = deps.encrypt(bytes, deps.getOperatorPubkey())
      },
    },
    {
      id: 'submitting',
      run: async () => {
        await deps.placeOrder({
          commitment: draft.commitment!,
          proof: draft.proof!,
          encryptedPayload: draft.encryptedPayload!,
        })
      },
    },
  ]
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_lib/entry/build-submission.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_lib/entry/build-submission.ts front/app/app/trade/_lib/entry/build-submission.test.ts && \
git commit -m "feat(#99): pure witness/payload builders and real stage steps"
```

---

## Task 5: Generalize the orchestrator to a step list

**Files:**
- Modify: `app/app/trade/_hooks/entry/useSubmitStages.ts`
- Modify: `app/app/trade/_hooks/entry/useSubmitStages.test.ts` (rewrite to new API)

The orchestrator stops hard-coding stage behaviour: it walks an injected
`StageStep[]`, stamps `stageStartedAtMs` for the elapsed ticker, and maps
thrown errors through an injectable `mapError` (default `mapSubmissionError`).
`buildMockSteps` reproduces today's mock behaviour exactly.

- [ ] **Step 1: Rewrite the test file** — replace the entire contents of `useSubmitStages.test.ts` with:

```ts
import { describe, expect, it, vi } from 'vitest'

import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/api-client'

import { STAGE_DURATIONS_MS, STAGE_ORDER, SUCCESS_HOLD_MS } from '../../_lib/entry/policy'
import {
  buildMockSteps,
  progressAtEndOfStage,
  progressAtStartOfStage,
  runSubmission,
  type SubmissionPhase,
  type SubmitPayload,
} from './useSubmitStages'

const PAYLOAD: SubmitPayload = { side: 'buy', price: '3000', size: '0.5' }

const instant = () => async () => {}

function record(phases: SubmissionPhase[]) {
  return (phase: SubmissionPhase) => {
    phases.push(phase)
  }
}

describe('progress helpers', () => {
  it('progressAtStartOfStage is 0 for the first stage and < 1 for all', () => {
    expect(progressAtStartOfStage('preparing')).toBe(0)
    for (const stage of STAGE_ORDER) {
      expect(progressAtStartOfStage(stage)).toBeGreaterThanOrEqual(0)
      expect(progressAtStartOfStage(stage)).toBeLessThan(1)
    }
  })
  it('progressAtEndOfStage is 1 for the last stage', () => {
    expect(progressAtEndOfStage('submitting')).toBe(1)
  })
})

describe('runSubmission', () => {
  it('emits each stage start+end in order, then success, then idle', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    const delay = instant()

    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay }), {
      onPhase: record(phases),
      delay,
      now: () => 1000,
    })

    const running = phases.filter((p) => p.kind === 'running') as Extract<
      SubmissionPhase,
      { kind: 'running' }
    >[]
    expect(running.map((p) => p.stage)).toEqual([
      'preparing',
      'preparing',
      'proving',
      'proving',
      'encrypting',
      'encrypting',
      'submitting',
      'submitting',
    ])
    // every running phase carries the injected clock stamp
    expect(running.every((p) => p.stageStartedAtMs === 1000)).toBe(true)

    const terminal = phases.slice(-2)
    expect(terminal[0].kind).toBe('success')
    expect(terminal[1].kind).toBe('idle')
  })

  it('calls placeOrder exactly once with the payload', async () => {
    const placeOrder = vi.fn()
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: () => {},
      delay: instant(),
    })
    expect(placeOrder).toHaveBeenCalledTimes(1)
    expect(placeOrder).toHaveBeenCalledWith(PAYLOAD)
  })

  it('emits an error phase (plain message, no detail) when a step throws', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn(() => {
      throw new Error('insufficient liquidity')
    })
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: record(phases),
      delay: instant(),
    })
    const terminal = phases[phases.length - 1]
    expect(terminal).toMatchObject({ kind: 'error', message: 'insufficient liquidity' })
    if (terminal.kind === 'error') expect(terminal.detail).toBeUndefined()
    expect(phases.some((p) => p.kind === 'success')).toBe(false)
  })

  it('attaches structured detail when a step throws a DarkPoolError', async () => {
    const phases: SubmissionPhase[] = []
    const steps = buildMockSteps(PAYLOAD, {
      placeOrder: () => {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED, 'slow down', {
          httpStatus: 429,
          retryAfter: '30',
          requestId: 'req-1',
        })
      },
      delay: instant(),
    })
    await runSubmission(steps, { onPhase: record(phases), delay: instant() })
    const terminal = phases[phases.length - 1]
    expect(terminal.kind).toBe('error')
    if (terminal.kind === 'error') {
      expect(terminal.detail?.retryAfter).toBe('30')
      expect(terminal.detail?.requestId).toBe('req-1')
    }
  })

  it('aborts before the next emission when shouldAbort returns true', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    let count = 0
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: record(phases),
      delay: instant(),
      shouldAbort: () => {
        count += 1
        return count > 1
      },
    })
    expect(placeOrder).not.toHaveBeenCalled()
    expect(phases.some((p) => p.kind === 'success' || p.kind === 'error')).toBe(false)
  })

  it('delays each stage by its duration, then SUCCESS_HOLD_MS', async () => {
    const delays: number[] = []
    const delay = async (ms: number) => {
      delays.push(ms)
    }
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder: vi.fn(), delay }), {
      onPhase: () => {},
      delay,
    })
    expect(delays).toEqual([
      STAGE_DURATIONS_MS.preparing,
      STAGE_DURATIONS_MS.proving,
      STAGE_DURATIONS_MS.encrypting,
      STAGE_DURATIONS_MS.submitting,
      SUCCESS_HOLD_MS,
    ])
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_hooks/entry/useSubmitStages.test.ts`
Expected: FAIL — `buildMockSteps` not exported / `runSubmission` signature mismatch.

- [ ] **Step 3: Rewrite `useSubmitStages.ts`** — replace the entire file with:

```ts
'use client'

// Drives the multi-stage submission (PREPARING WITNESS → GENERATING PROOF →
// ENCRYPTING → SUBMITTING → success/error). The orchestrator walks an
// injected ordered list of StageStep — the mock path (buildMockSteps) and
// the real path (createRealSteps, see _lib/entry/build-submission.ts) both
// assemble that list, so one node-testable state machine drives both.
//
// `stageStartedAtMs` is stamped on each running emission so the view can
// show real elapsed seconds per stage. Errors are routed through an
// injectable `mapError` (default mapSubmissionError) so a DarkPoolError
// becomes specific copy + structured detail for the inline error area.

import { useCallback, useLayoutEffect, useRef, useState } from 'react'

import {
  STAGE_DURATIONS_MS,
  STAGE_ORDER,
  STAGE_TOTAL_MS,
  SUCCESS_HOLD_MS,
  type SubmitStageId,
} from '../../_lib/entry/policy'
import {
  randomHex,
  type StageStep,
} from '../../_lib/entry/build-submission'
import { mapSubmissionError, type SubmitErrorDetail } from '../../_lib/entry/submit-error'

export type { StageStep } from '../../_lib/entry/build-submission'

export type SubmissionPhase =
  | { kind: 'idle' }
  | { kind: 'running'; stage: SubmitStageId; progress: number; stageStartedAtMs?: number }
  | { kind: 'success' }
  | { kind: 'error'; message: string; detail?: SubmitErrorDetail }

export interface SubmitPayload {
  side: 'buy' | 'sell'
  price: string
  size: string
}

export interface RunSubmissionOptions {
  onPhase: (phase: SubmissionPhase) => void
  /** Resolves after `ms`. Defaults to setTimeout. */
  delay?: (ms: number) => Promise<void>
  /** Monotonic clock for the elapsed ticker. Defaults to Date.now. */
  now?: () => number
  /** `true` aborts the run between awaits (drops stale runs). */
  shouldAbort?: () => boolean
  /** Maps a thrown value to an error phase. Defaults to mapSubmissionError. */
  mapError?: (err: unknown) => { message: string; detail?: SubmitErrorDetail }
}

export function progressAtStartOfStage(stage: SubmitStageId): number {
  let elapsed = 0
  for (const id of STAGE_ORDER) {
    if (id === stage) break
    elapsed += STAGE_DURATIONS_MS[id]
  }
  return elapsed / STAGE_TOTAL_MS
}

export function progressAtEndOfStage(stage: SubmitStageId): number {
  let elapsed = 0
  for (const id of STAGE_ORDER) {
    elapsed += STAGE_DURATIONS_MS[id]
    if (id === stage) break
  }
  return elapsed / STAGE_TOTAL_MS
}

const defaultDelay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))
const defaultNow = () => Date.now()

/**
 * Mock steps: a fixed delay per stage (matching STAGE_DURATIONS_MS) with the
 * mock placeOrder fired during `submitting`. Reproduces the F1.9 behaviour
 * for the demo/Storybook path. `prove`, when supplied, replaces the proving
 * delay (legacy parity).
 */
export function buildMockSteps(
  payload: SubmitPayload,
  opts: {
    placeOrder: (payload: SubmitPayload) => void | Promise<void>
    prove?: (witness: {
      commitment_key: string
      side: number
      price: string
      size: string
      salt_hex: string
    }) => Promise<unknown>
    delay?: (ms: number) => Promise<void>
  }
): StageStep[] {
  const delay = opts.delay ?? defaultDelay
  return STAGE_ORDER.map((id) => ({
    id,
    run: async () => {
      if (id === 'proving' && opts.prove) {
        await opts.prove({
          commitment_key: randomHex(32),
          side: payload.side === 'buy' ? 0 : 1,
          price: payload.price,
          size: payload.size,
          salt_hex: randomHex(32),
        })
        return
      }
      if (id === 'submitting') {
        await Promise.resolve(opts.placeOrder(payload))
      }
      await delay(STAGE_DURATIONS_MS[id])
    },
  }))
}

/**
 * Pure orchestrator. Emits each phase change through `onPhase` and resolves
 * once the run lands on a terminal state and the post-success hold elapses.
 */
export async function runSubmission(steps: StageStep[], opts: RunSubmissionOptions): Promise<void> {
  const delay = opts.delay ?? defaultDelay
  const now = opts.now ?? defaultNow
  const mapError = opts.mapError ?? mapSubmissionError
  const aborted = () => (opts.shouldAbort ? opts.shouldAbort() : false)

  try {
    for (const step of steps) {
      if (aborted()) return
      opts.onPhase({
        kind: 'running',
        stage: step.id,
        progress: progressAtStartOfStage(step.id),
        stageStartedAtMs: now(),
      })

      await step.run({ aborted })

      if (aborted()) return
      opts.onPhase({
        kind: 'running',
        stage: step.id,
        progress: progressAtEndOfStage(step.id),
        stageStartedAtMs: now(),
      })
    }

    if (aborted()) return
    opts.onPhase({ kind: 'success' })

    await delay(SUCCESS_HOLD_MS)
    if (aborted()) return
    opts.onPhase({ kind: 'idle' })
  } catch (err) {
    if (aborted()) return
    const { message, detail } = mapError(err)
    opts.onPhase({ kind: 'error', message, detail })
  }
}

export interface UseSubmitStagesParams {
  /** Build the ordered steps for a given form payload (mock or real). */
  buildSteps: (payload: SubmitPayload) => StageStep[]
  onSuccess?: (payload: SubmitPayload) => void
  onError?: (error: Error) => void
  delay?: (ms: number) => Promise<void>
  now?: () => number
}

export interface UseSubmitStagesResult {
  phase: SubmissionPhase
  isRunning: boolean
  submit: (payload: SubmitPayload) => Promise<void>
  reset: () => void
}

export function useSubmitStages(params: UseSubmitStagesParams): UseSubmitStagesResult {
  const [phase, setPhase] = useState<SubmissionPhase>({ kind: 'idle' })
  const runIdRef = useRef(0)
  const paramsRef = useRef(params)
  useLayoutEffect(() => {
    paramsRef.current = params
  })

  const reset = useCallback(() => {
    runIdRef.current += 1
    setPhase({ kind: 'idle' })
  }, [])

  const submit = useCallback(async (payload: SubmitPayload) => {
    const myRunId = ++runIdRef.current
    const isStale = () => runIdRef.current !== myRunId
    const p = paramsRef.current

    await runSubmission(p.buildSteps(payload), {
      delay: p.delay,
      now: p.now,
      shouldAbort: isStale,
      onPhase: (next) => {
        if (isStale()) return
        setPhase(next)
        if (next.kind === 'success') p.onSuccess?.(payload)
        if (next.kind === 'error') p.onError?.(new Error(next.message))
      },
    })
  }, [])

  return {
    phase,
    isRunning: phase.kind === 'running',
    submit,
    reset,
  }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_hooks/entry/useSubmitStages.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_hooks/entry/useSubmitStages.ts front/app/app/trade/_hooks/entry/useSubmitStages.test.ts && \
git commit -m "feat(#99): generalize submit orchestrator to an injected step list"
```

---

## Task 6: Real-submission hook (`useRealSubmission.ts`)

**Files:**
- Create: `app/app/trade/_hooks/entry/useRealSubmission.ts`

This hook supplies the real dependencies (trader id, prover, operator pubkey,
SDK client) to `createRealSteps`, and exposes the live proving percentage for
the progress bar. Its logic is thin; the testable pieces (builders, step
sequencing) are already covered in Task 4. No new test file — Task 8's
composition test exercises the wiring in mock mode, and the real branch is
guarded by the gate.

- [ ] **Step 1: Implement** — `useRealSubmission.ts`:

```ts
'use client'

import { useCallback, useRef } from 'react'
import { create } from '@bufbuild/protobuf'

import { config } from '@/lib/config'
import { encryptOrder, serializeOrder, useOperatorPubkey } from '@/lib/crypto'
import { useProver } from '@/lib/prover'
import { PlaceOrderRequestSchema } from '@/lib/sdk'
import { useDarkPoolClient } from '@/lib/api-client'
import { useTraderId } from '@/lib/wallet/hooks'

import { createRealSteps, randomHex, type StageStep } from '../../_lib/entry/build-submission'
import { ORDER_PAIR, ORDER_TTL_NS } from '../../_lib/entry/policy'
import type { SubmitPayload } from './useSubmitStages'

export interface UseRealSubmissionResult {
  buildSteps: (payload: SubmitPayload) => StageStep[]
  /** Live proving percentage (0-100) for the progress bar, or null. */
  provingPct: number | null
}

export function useRealSubmission(): UseRealSubmissionResult {
  const trader = useTraderId()
  const { prove, progress } = useProver()
  const client = useDarkPoolClient()

  // The operator pubkey is fetched via TanStack Query; the steps read the
  // latest value through a ref so a slow fetch doesn't capture a stale
  // undefined. `enabled` is false in mock mode, so this is inert there.
  const pubkeyQuery = useOperatorPubkey(config.apiUrl, config.useMocks)
  const pubkeyRef = useRef<Uint8Array | undefined>(undefined)
  pubkeyRef.current = pubkeyQuery.data

  const buildSteps = useCallback(
    (payload: SubmitPayload): StageStep[] =>
      createRealSteps({
        trader: trader ?? '',
        pair: ORDER_PAIR,
        ttlNs: ORDER_TTL_NS,
        side: payload.side,
        price: payload.price,
        size: payload.size,
        randomHex,
        getOperatorPubkey: () => {
          const pk = pubkeyRef.current
          if (!pk) throw new Error('Operator key is still loading. Try again in a moment.')
          return pk
        },
        prove: (witness) => prove(witness),
        serialize: serializeOrder,
        encrypt: encryptOrder,
        placeOrder: (trio) => client.placeOrder(create(PlaceOrderRequestSchema, trio)),
      }),
    [trader, prove, client]
  )

  const provingPct = progress ? progress.pct : null

  return { buildSteps, provingPct }
}
```

- [ ] **Step 2: Typecheck**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_hooks/entry/useRealSubmission.ts && \
git commit -m "feat(#99): assemble real submission steps from React hooks"
```

---

## Task 7: View — elapsed ticker, proving bar, inline error

**Files:**
- Modify: `app/app/trade/_components/entry/ProveSubmitStages.tsx`

Three changes: (a) the running button label shows real elapsed seconds when
`stageStartedAtMs` is set; (b) the progress bar follows `provingPct` during the
proving stage; (c) a new `SubmitError` component renders the inline error area
(specific message, 429 retry-after, 5xx collapsible `x-request-id`).

- [ ] **Step 1: Replace the file** with:

```tsx
'use client'

// Visual layer for the multi-stage submission. Renders the place-button
// label (which mutates through the stages, now with real elapsed seconds),
// the thin progress bar underneath it, and the inline error area below.
// Presentational only — state comes from useSubmitStages.

import * as React from 'react'

import { cn } from '@/components/ui/cn'

import { STAGE_LABELS, type SubmitStageId } from '../../_lib/entry/policy'
import type { SubmissionPhase } from '../../_hooks/entry/useSubmitStages'

export interface PlaceButtonProps {
  /** What the button reads when idle (e.g. "BUY · WETH"). */
  idleLabel: string
  phase: SubmissionPhase
  disabled?: boolean
  onClick: () => void
  /** Lime accent surface when the form is valid and idle. */
  accent: boolean
  /** Live proving percentage (0-100), used for the proving-stage bar. */
  provingPct?: number | null
  /** Injectable clock for tests; defaults to Date.now. */
  now?: () => number
}

/** Live seconds since the current running stage started (0 when unknown). */
function useStageElapsed(phase: SubmissionPhase, now: () => number): number {
  const [, force] = React.useState(0)
  const running = phase.kind === 'running' && phase.stageStartedAtMs !== undefined
  React.useEffect(() => {
    if (!running) return
    const id = setInterval(() => force((n) => n + 1), 100)
    return () => clearInterval(id)
  }, [running, phase.kind === 'running' ? phase.stage : null])

  if (phase.kind !== 'running' || phase.stageStartedAtMs === undefined) return 0
  return Math.max(0, (now() - phase.stageStartedAtMs) / 1000)
}

export function PlaceButton({
  idleLabel,
  phase,
  disabled,
  onClick,
  accent,
  provingPct,
  now = Date.now,
}: PlaceButtonProps) {
  const elapsed = useStageElapsed(phase, now)
  const label = labelFor(phase, idleLabel, elapsed)
  const isRunning = phase.kind === 'running'
  const showSuccess = phase.kind === 'success'

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={onClick}
        disabled={disabled || isRunning}
        aria-busy={isRunning || undefined}
        aria-live="polite"
        className={cn(
          'relative flex h-12 items-center justify-center px-8',
          'font-mono uppercase tracking-[0.15em] text-[11px] font-medium leading-none',
          'transition-[color,background-color,box-shadow] duration-150 ease-out',
          'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
          accent
            ? 'bg-brand-accent text-brand-on-accent hover:shadow-accent-glow'
            : 'bg-transparent text-brand-muted border border-brand-border shadow-[inset_0_0_0_1px_#0C0C12]',
          'disabled:cursor-not-allowed',
          accent
            ? 'disabled:bg-brand-border disabled:text-brand-muted disabled:shadow-none'
            : 'disabled:text-brand-muted'
        )}
      >
        <span className="block">{label}</span>
      </button>
      <ProgressBar phase={phase} success={showSuccess} provingPct={provingPct} />
    </div>
  )
}

function labelFor(phase: SubmissionPhase, idleLabel: string, elapsedSecs: number): string {
  switch (phase.kind) {
    case 'idle':
      return idleLabel
    case 'running': {
      const base = STAGE_LABELS[phase.stage]
      // Show real elapsed seconds once the stage start is known.
      return phase.stageStartedAtMs !== undefined
        ? `${base} · ${elapsedSecs.toFixed(1)}s …`
        : `${base} …`
    }
    case 'success':
      return 'ORDER PLACED'
    case 'error':
      return 'TRY AGAIN'
  }
}

function ProgressBar({
  phase,
  success,
  provingPct,
}: {
  phase: SubmissionPhase
  success: boolean
  provingPct?: number | null
}) {
  const width = progressWidth(phase, success, provingPct)
  const visible = phase.kind === 'running' || success
  return (
    <div
      aria-hidden
      className={cn(
        'h-[2px] w-full overflow-hidden bg-brand-border/0',
        'transition-opacity duration-150',
        visible ? 'opacity-100' : 'opacity-0'
      )}
    >
      <div
        data-testid="place-progress"
        className="h-full bg-brand-accent transition-[width] duration-150 ease-out"
        style={{ width: `${(width * 100).toFixed(2)}%` }}
      />
    </div>
  )
}

function progressWidth(
  phase: SubmissionPhase,
  success: boolean,
  provingPct?: number | null
): number {
  if (success) return 1
  if (phase.kind !== 'running') return 0
  // During proving, follow the real prover percentage when available.
  if (phase.stage === 'proving' && provingPct != null) {
    return Math.min(1, Math.max(0, provingPct / 100))
  }
  return phase.progress
}

export function StageReadout({ phase }: { phase: SubmissionPhase }) {
  if (phase.kind !== 'running') return null
  return (
    <span className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted">
      {STAGE_LABELS[phase.stage as SubmitStageId]} …
    </span>
  )
}

/**
 * Inline error area shown below the place button when a submission fails.
 * Specific message per tonic code; on 429 a retry-after hint; on 5xx a
 * collapsible technical block carrying the x-request-id (C7).
 */
export function SubmitError({ phase }: { phase: SubmissionPhase }) {
  if (phase.kind !== 'error') return null
  const detail = phase.detail
  const showTechnical = detail != null && detail.httpStatus != null && detail.httpStatus >= 500

  return (
    <div role="alert" className="flex flex-col gap-2 border border-brand-border px-3 py-2">
      <p className="font-mono text-body-sm text-brand-fg">{phase.message}</p>

      {detail?.retryAfter && (
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted">
          Retry after {detail.retryAfter}s
        </p>
      )}

      {showTechnical && (
        <details className="font-mono text-[10px] text-brand-muted">
          <summary className="cursor-pointer uppercase tracking-[0.2em]">
            [ TECHNICAL DETAIL ]
          </summary>
          <dl className="mt-2 flex flex-col gap-1">
            <div className="flex gap-2">
              <dt className="text-brand-muted">request-id</dt>
              <dd className="text-brand-fg">{detail?.requestId ?? '—'}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="text-brand-muted">code</dt>
              <dd className="text-brand-fg">
                {detail?.codeName} ({detail?.httpStatus})
              </dd>
            </div>
            {detail?.serverMessage && (
              <div className="flex gap-2">
                <dt className="text-brand-muted">message</dt>
                <dd className="text-brand-fg">{detail.serverMessage}</dd>
              </div>
            )}
          </dl>
        </details>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Typecheck**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx tsc --noEmit`
Expected: PASS. (The stories' `PlaceButtonStages` still compiles — `provingPct`/`now` are optional and the running phases omit `stageStartedAtMs`, so labels read `GENERATING PROOF …`.)

- [ ] **Step 3: Run the entry component tests to confirm no regression**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_components/entry/OrderEntry.test.tsx`
Expected: PASS (unchanged behaviour at this point).

- [ ] **Step 4: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_components/entry/ProveSubmitStages.tsx && \
git commit -m "feat(#99): real elapsed labels, proving-pct bar, inline SubmitError"
```

---

## Task 8: Wire `OrderEntry` — mock-vs-real gate + inline error

**Files:**
- Modify: `app/app/trade/_components/entry/OrderEntry.tsx`
- Modify: `app/app/trade/_components/entry/OrderEntry.test.tsx`

`OrderEntry` picks the step builder once: real when no `placeOrder` prop is
injected AND the placeOrder RPC is not mocked; otherwise the mock builder
(today's behaviour). The error toast is dropped in favour of the inline
`SubmitError`; the success toast stays.

- [ ] **Step 1: Add the failing composition tests** — append inside the existing `describe('OrderEntry composition', ...)` block in `OrderEntry.test.tsx`, after the last `it(...)`:

```tsx
  it('does not render an inline error in the idle/default state', () => {
    walletStore.connect()
    const html = renderPanel()
    expect(html).not.toContain('[ TECHNICAL DETAIL ]')
    expect(html).not.toContain('role="alert"')
    // sanity: the place button is present
    expect(html).toContain('order-entry-form')
  })
```

(Also add the import if not present — it already imports `OrderEntry`; no new import needed.)

- [ ] **Step 2: Run, verify it passes pre-change (guard test) and the suite is green**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_components/entry/OrderEntry.test.tsx`
Expected: PASS — this is a guard that the refactor below keeps the idle render clean. (Note: the existing `role="alert"` field errors render only when there is a validation error with a connected wallet and filled-invalid inputs; in the default connected render there are none, so the assertion holds. If it fails here, the panel already renders an alert in idle and the assertion must be scoped — but it should pass.)

- [ ] **Step 3: Rewrite `OrderEntry.tsx`** — replace the entire file with:

```tsx
'use client'

// Order entry composition root.
//
// Submission gate: when the placeOrder RPC is mocked (config.useMocks or
// NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER) — or a `placeOrder` prop is injected by
// Storybook/tests — the staged MOCK pipeline runs (fixed delays, mock-store
// push). Otherwise the REAL pipeline runs: build witness → WASM prove →
// ECIES encrypt → POST /v1/orders (#99). Failures surface inline below the
// button via <SubmitError>; success keeps the toast.

import * as React from 'react'

import { config } from '@/lib/config'
import { methodOverridesFromEnv } from '@/lib/api-client'
import { mockStore } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { Decimal } from '@/lib/units'
import { useInternalBalances, useWallet } from '@/lib/wallet/hooks'

import { useToast } from '@/components/ui/use-toast'

import { BuySellTabs } from './BuySellTabs'
import { errorMessage } from '../../_lib/entry/errors'
import { DecimalInput } from './inputs'
import { BASE_TOKEN, FEE_BPS, QUOTE_TOKEN } from '../../_lib/entry/policy'
import { PlaceButton, SubmitError } from './ProveSubmitStages'
import { TotalRow } from './TotalRow'
import { buildMockSteps, useSubmitStages, type SubmitPayload } from '../../_hooks/entry/useSubmitStages'
import { useRealSubmission } from '../../_hooks/entry/useRealSubmission'
import { useOrderForm } from '../../_hooks/entry/useOrderForm'
import type { OrderSide } from '../../_lib/entry/validate'

export interface OrderEntryHandle {
  fill: (price: string, side?: OrderSide) => void
}

export interface OrderEntryProps {
  /** Inject the mock-store mutation (Storybook/tests). Forces the mock path. */
  placeOrder?: (payload: SubmitPayload) => void
  /** Injectable wait for the staged mock submission. */
  delay?: (ms: number) => Promise<void>
}

const FEE_FACTOR = new Decimal(1).plus(new Decimal(FEE_BPS).div(10_000))

/** True when the real pipeline should run for placeOrder. */
function realPlaceOrderEnabled(): boolean {
  const override = methodOverridesFromEnv().placeOrder
  const mocked = override ?? config.useMocks
  return !mocked
}

export const OrderEntry = React.forwardRef<OrderEntryHandle, OrderEntryProps>(function OrderEntry(
  { placeOrder, delay },
  ref
) {
  const { isConnected } = useWallet()
  const balances = useInternalBalances()
  const { toast } = useToast()
  const formRef = React.useRef<HTMLFormElement>(null)
  const headerId = React.useId()
  const priceErrorId = React.useId()
  const sizeErrorId = React.useId()
  const formErrorId = React.useId()

  const form = useOrderForm({
    isConnected,
    baseBalance: balances.weth,
    quoteBalance: balances.usdc,
  })
  const formStateRef = React.useRef(form)
  formStateRef.current = form

  // Real deps are read unconditionally (hooks rules); inert in mock mode.
  const real = useRealSubmission()

  // Mock path: injected placeOrder, else the singleton mock store.
  const effectiveMockPlaceOrder = React.useCallback(
    (payload: SubmitPayload) => {
      if (placeOrder) {
        placeOrder(payload)
        return
      }
      mockStore.getState().placeOrder({
        side: payload.side === 'buy' ? Side.BUY : Side.SELL,
        price: payload.price,
        size: payload.size,
      })
    },
    [placeOrder]
  )

  const useReal = !placeOrder && realPlaceOrderEnabled()

  const buildSteps = React.useCallback(
    (payload: SubmitPayload) =>
      useReal
        ? real.buildSteps(payload)
        : buildMockSteps(payload, { placeOrder: effectiveMockPlaceOrder, delay }),
    [useReal, real, effectiveMockPlaceOrder, delay]
  )

  const submit = useSubmitStages({
    buildSteps,
    delay,
    onSuccess: () => {
      toast({
        title: 'Order placed',
        description: 'Pending next auction.',
        variant: 'accent',
      })
      form.reset()
    },
    // Errors render inline via <SubmitError> (no toast) per #99 design.
  })

  React.useImperativeHandle(
    ref,
    () => ({
      fill: (price: string, nextSide?: OrderSide) => {
        formStateRef.current.fillFromLevel(price, nextSide)
      },
    }),
    []
  )

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!form.validation.ok || submit.isRunning) return
    void submit.submit({ side: form.side, price: form.price, size: form.size })
  }

  const handleMax = () => {
    try {
      if (form.side === 'sell') {
        form.setSize(balances.weth)
        return
      }
      if (form.price.trim() === '') return
      const priceD = new Decimal(form.price)
      if (priceD.lte(0)) return
      const quoteD = new Decimal(balances.usdc)
      const maxSize = quoteD.div(FEE_FACTOR).div(priceD)
      form.setSize(maxSize.toDecimalPlaces(4, Decimal.ROUND_DOWN).toFixed())
    } catch {
      /* invalid input — leave the size field as-is */
    }
  }

  const priceError = form.validation.errors.price
  const sizeError = form.validation.errors.size
  const formError = form.validation.errors.form

  const idleLabel = `${form.side === 'buy' ? '[ BUY' : '[ SELL'} · ${BASE_TOKEN} ]`
  const accentActive = form.validation.ok && submit.phase.kind !== 'running'

  return (
    <section
      aria-labelledby={headerId}
      className="flex h-full flex-col border border-brand-border bg-brand-surface"
    >
      <header className="flex h-9 items-center border-b border-brand-border px-4">
        <span
          id={headerId}
          className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
        >
          [ ORDER ENTRY ]
        </span>
      </header>
      <form
        ref={formRef}
        id="order-entry-form"
        onSubmit={handleSubmit}
        aria-describedby={formError ? formErrorId : undefined}
        className="flex flex-1 flex-col gap-4 p-4"
        noValidate
      >
        <BuySellTabs value={form.side} onChange={form.setSide} disabled={submit.isRunning} />

        <DecimalInput
          id="order-entry-price"
          label={`[ PRICE · ${QUOTE_TOKEN} ]`}
          unit={QUOTE_TOKEN}
          value={form.price}
          onChange={form.setPrice}
          placeholder="0.00"
          disabled={submit.isRunning}
          invalid={!!priceError}
          errorId={priceError ? priceErrorId : undefined}
        />
        {priceError && (
          <p id={priceErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(priceError)}
          </p>
        )}

        <DecimalInput
          id="order-entry-size"
          label={`[ SIZE · ${BASE_TOKEN} ]`}
          unit={BASE_TOKEN}
          value={form.size}
          onChange={form.setSize}
          placeholder="0.0000"
          disabled={submit.isRunning}
          invalid={!!sizeError}
          errorId={sizeError ? sizeErrorId : undefined}
          rightSlot={
            <button
              type="button"
              onClick={handleMax}
              disabled={submit.isRunning || !isConnected}
              className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted hover:text-brand-fg transition-colors disabled:cursor-not-allowed disabled:opacity-50"
              aria-label="Set size to maximum"
            >
              [ MAX ]
            </button>
          }
        />
        {sizeError && (
          <p id={sizeErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(sizeError)}
          </p>
        )}

        <TotalRow price={form.price} size={form.size} />

        {formError && (
          <p id={formErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(formError)}
          </p>
        )}

        <PlaceButton
          idleLabel={idleLabel}
          phase={submit.phase}
          disabled={!form.validation.ok}
          accent={accentActive}
          provingPct={useReal ? real.provingPct : undefined}
          onClick={() => formRef.current?.requestSubmit()}
        />

        <SubmitError phase={submit.phase} />
      </form>
    </section>
  )
})
```

- [ ] **Step 4: Run the composition tests**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run app/app/trade/_components/entry/OrderEntry.test.tsx`
Expected: PASS.

Note on the test environment: these tests render with `renderToStaticMarkup`
and never connect a real client, and the default test env has
`NEXT_PUBLIC_USE_MOCKS` truthy, so `realPlaceOrderEnabled()` is false and the
mock path is used — no worker/fetch is created. If `config` import throws in
the test env because required env vars are absent, that is a pre-existing
condition (the suite already imports the wallet store and renders `OrderEntry`,
which transitively imports `config` via the SDK provider chain only at runtime);
if a new failure appears here, stub `methodOverridesFromEnv`/`config` is NOT
needed — instead confirm `front/.env.test`/vitest env defines
`NEXT_PUBLIC_USE_MOCKS=true`. Do not weaken the gate to make a test pass.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_components/entry/OrderEntry.tsx front/app/app/trade/_components/entry/OrderEntry.test.tsx && \
git commit -m "feat(#99): gate OrderEntry between mock and real submission pipelines"
```

---

## Task 9: Exports, full suite, typecheck, lint

**Files:**
- Modify: `app/app/trade/_lib/entry/index.ts`, `app/app/trade/_components/entry/index.ts` (and `_hooks` barrel if present)

- [ ] **Step 1: Add new exports** — append to `_lib/entry/index.ts`:

```ts
export {
  buildOrderPayload,
  buildWitness,
  createRealSteps,
  randomHex,
  type RealStepDeps,
  type StageStep,
} from './build-submission'
export {
  mapSubmissionError,
  submitErrorMessage,
  toSubmitErrorDetail,
  type SubmitErrorDetail,
} from './submit-error'
export { ORDER_PAIR, ORDER_TTL_NS } from './policy'
```

- [ ] **Step 2: Add component/hook exports** — append to `_components/entry/index.ts`:

```ts
export { SubmitError } from './ProveSubmitStages'
export { useRealSubmission } from '../../_hooks/entry/useRealSubmission'
export { buildMockSteps } from '../../_hooks/entry/useSubmitStages'
```

- [ ] **Step 3: Full test suite**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx vitest run`
Expected: PASS (all suites, including the pre-existing ones).

- [ ] **Step 4: Typecheck**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Lint**

Run: `cd /home/mario/darkpool-wt/99-order-e2e/front && npm run lint`
Expected: PASS (no new errors in touched files).

- [ ] **Step 6: Commit**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git add front/app/app/trade/_lib/entry/index.ts front/app/app/trade/_components/entry/index.ts && \
git commit -m "chore(#99): export real-submission builders, error mapper, SubmitError"
```

---

## Task 10: Open the PR

- [ ] **Step 1: Push and open PR**

```bash
cd /home/mario/darkpool-wt/99-order-e2e && \
git push -u origin feat/issue-99-order-e2e && \
gh pr create --title "[I2.10] End-to-end real order placement" --body "$(cat <<'EOF'
Closes #99

Replaces the F1.9 mock submit internals with the real pipeline — build witness → WASM prove (#98) → ECIES encrypt (#96) → POST /v1/orders — behind the existing mock gate. The F1.9 layout and the mock/demo path are unchanged.

## What changed
- Single orchestrator now walks an injected ordered `StageStep[]`; mock and real both assemble that list (one node-testable state machine).
- Real pipeline mints **one** `commitment_key` per order and threads it into both the prover witness and the encrypted `DecryptedOrder` payload (no salt sent — the operator recomputes the canonical commitment).
- Stage labels show **real elapsed seconds**; the proving bar follows `useProver().progress.pct`.
- All tonic `Code` variants from `crates/dp-api/src/rest.rs` `ApiError` map to specific inline messages. **429** surfaces `retry-after`; **5xx** shows a collapsible technical block with `x-request-id` (C7).

## Scope-crossing (flagged)
- Small additive edit to `front/lib/sdk/client.ts`: `DarkPoolError` now captures the `x-request-id` and `retry-after` response headers (previously dropped). No behaviour change for existing callers.

## Known limitation (backend follow-up, not in this PR)
The per-order WASM proof is a placeholder: the circuit derives `trader_id` from `commitment_key`, while the operator derives it from the real trader address (different salts), so the proof's commitment would not match an operator *content* check. The operator does not content-verify the client commitment today, so real orders are accepted. Binding the proof to the real trader identity is a prover/circuit follow-up (those crates are out of #99's scope).
EOF
)"
```

Expected: PR created with `Closes #99`.

---

## Self-Review

**Spec coverage:**
- F1.9 submit replaced with witness→prove→encrypt→placeOrder → Tasks 4, 6, 8. ✓
- Stage labels reflect real elapsed time → Task 5 (`stageStartedAtMs`) + Task 7 (ticker). ✓
- All tonic Code variants mapped → Task 3 (`MESSAGES` covers all 17 names; test asserts each). ✓
- 429 retry-after → Tasks 1, 3, 7. ✓
- 5xx collapsible x-request-id → Tasks 1, 3, 7. ✓
- Mock path intact behind gate → Tasks 5 (`buildMockSteps`), 8 (`realPlaceOrderEnabled`). ✓
- §7 SDK header capture → Task 1. ✓
- Known ZK limitation surfaced → Task 10 PR body. ✓

**Placeholder scan:** No TBD/TODO; every code step is complete. ✓

**Type consistency:**
- `StageStep` defined in `build-submission.ts` (Task 4), re-exported from `useSubmitStages.ts` (Task 5), consumed in Tasks 6/8. ✓
- `SubmitErrorDetail` defined in `submit-error.ts` (Task 3), referenced by `SubmissionPhase.error.detail` (Task 5) and `SubmitError` view (Task 7). ✓
- `buildMockSteps(payload, opts)` signature identical in Task 5 definition and Task 8 call. ✓
- `createRealSteps(deps)` `RealStepDeps` fields match the Task 6 hook's call. ✓
- `WitnessInput` (`{commitment_key, side, price, size, salt_hex}`) matches `lib/prover` and `dp-zk-wasm/src/lib.rs`. ✓
- `DecryptedOrderPayload` (`{trader, pair, side, price, size, commitment_key, ttl}`) matches `lib/crypto/order-payload.ts` and `dp-crypto/src/decrypted_order.rs`. ✓
