# [I2.6 / #95] Auction Streaming Upgrade — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the auction tape from 2 s REST polling to a live SSE stream that seamlessly degrades back to polling on disconnect, with jittered exponential-backoff reconnection.

**Architecture:** Three layers with clean seams. (1) `RestClient.streamAuctions` becomes a single-connection SSE reader (fetch + ReadableStream) yielding `AuctionEvent`s. (2) `useAuctionStream` owns the reconnect state machine. (3) `useAuctionFeed` merges live events with the existing I2.5 history poll (re-enabled only while not-live) and feeds the Tape, which shows a LIVE/DELAYED badge.

**Tech Stack:** TypeScript, Next.js 14 App Router, `@bufbuild/protobuf` (proto codegen), TanStack Query, Vitest + `@testing-library/react` (jsdom), Tailwind v3.

**Spec:** `docs/superpowers/specs/2026-05-31-auction-streaming-upgrade-design.md`

**Worktree:** `/home/mario/darkpool-wt/95-auction-streaming` on `feat/issue-95-auction-streaming`. All commands below run from `front/` unless noted (`cd front` first). Test runner: `npx vitest run <path>`.

---

## File Structure

| File | Responsibility |
|---|---|
| `front/lib/sdk/client.ts` | `streamAuctions` body: SSE fetch + frame parser (private helpers). |
| `front/lib/sdk/client.test.ts` | SSE transport tests (replaces the UNIMPLEMENTED test). |
| `front/app/app/trade/_lib/tape/backoff.ts` | Pure full-jitter exponential backoff. |
| `front/app/app/trade/_lib/tape/backoff.test.ts` | Backoff unit tests. |
| `front/app/app/trade/_lib/tape/feed.ts` | Pure merge reducer + `AuctionEvent`→`AuctionSummary`. |
| `front/app/app/trade/_lib/tape/feed.test.ts` | Feed reducer unit tests. |
| `front/app/app/trade/_hooks/tape/useAuctionHistory.ts` | Allow `refetchIntervalMs: number \| false`. |
| `front/app/app/trade/_hooks/tape/useAuctionHistory.test.ts` | Poll-toggle test. |
| `front/app/app/trade/_hooks/tape/useAuctionStream.ts` | Reconnect FSM over the SDK stream. |
| `front/app/app/trade/_hooks/tape/useAuctionStream.test.tsx` | FSM tests (fake timers + fake client). |
| `front/app/app/trade/_hooks/tape/useAuctionFeed.ts` | Compose stream + history; expose `{auctions,status}`. |
| `front/app/app/trade/_hooks/tape/useAuctionFeed.test.tsx` | Integration tests. |
| `front/app/app/trade/_components/tape/StreamStatus.tsx` | LIVE/DELAYED badge. |
| `front/app/app/trade/_components/tape/StreamStatus.test.tsx` | Badge tests. |
| `front/app/app/trade/_components/tape/StreamStatus.stories.tsx` | Storybook variants. |
| `front/app/app/trade/_components/tape/Countdown.tsx` | Optional `status` prop hosts the badge. |
| `front/app/app/trade/_components/tape/Tape.tsx` | Consume `useAuctionFeed`; pass `status` to Countdown. |

---

## Task 1: Backoff utility (pure)

**Files:**
- Create: `front/app/app/trade/_lib/tape/backoff.ts`
- Test: `front/app/app/trade/_lib/tape/backoff.test.ts`

- [ ] **Step 1: Write the failing test**

Create `front/app/app/trade/_lib/tape/backoff.test.ts`:

```ts
import { describe, expect, it } from 'vitest'

import { backoffDelay, BACKOFF_BASE_MS, BACKOFF_CAP_MS } from './backoff'

describe('backoffDelay', () => {
  it('attempt 0 draws from [0, base)', () => {
    expect(backoffDelay(0, { random: () => 0 })).toBe(0)
    expect(backoffDelay(0, { random: () => 0.999 })).toBe(Math.floor(0.999 * BACKOFF_BASE_MS))
    expect(backoffDelay(0, { random: () => 0.999 })).toBeLessThan(BACKOFF_BASE_MS)
  })

  it('ceiling doubles each attempt until the cap', () => {
    const half = (n: number) => backoffDelay(n, { random: () => 0.5 })
    expect(half(0)).toBe(BACKOFF_BASE_MS / 2) // 500
    expect(half(1)).toBe(BACKOFF_BASE_MS) // 1000
    expect(half(2)).toBe(BACKOFF_BASE_MS * 2) // 2000
    expect(half(10)).toBe(BACKOFF_CAP_MS / 2) // capped: 15000
  })

  it('never exceeds the cap, even with random→1', () => {
    const d = backoffDelay(50, { random: () => 0.999999 })
    expect(d).toBeLessThan(BACKOFF_CAP_MS)
  })

  it('treats negative attempts as 0', () => {
    expect(backoffDelay(-5, { random: () => 0 })).toBe(0)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd front && npx vitest run app/app/trade/_lib/tape/backoff.test.ts`
Expected: FAIL — `Failed to resolve import "./backoff"`.

- [ ] **Step 3: Write minimal implementation**

Create `front/app/app/trade/_lib/tape/backoff.ts`:

```ts
// Full-jitter exponential backoff (AWS "Exponential Backoff and Jitter").
// Returns a delay in [0, ceiling) where ceiling = min(cap, base * 2**attempt).
// Full jitter (random across the whole window) decorrelates reconnect storms
// when many clients drop at once — better than fixed or equal jitter.

export const BACKOFF_BASE_MS = 1000
export const BACKOFF_CAP_MS = 30_000

export interface BackoffOptions {
  baseMs?: number
  capMs?: number
  /** Injectable for deterministic tests. Defaults to Math.random. */
  random?: () => number
}

export function backoffDelay(attempt: number, opts: BackoffOptions = {}): number {
  const base = opts.baseMs ?? BACKOFF_BASE_MS
  const cap = opts.capMs ?? BACKOFF_CAP_MS
  const random = opts.random ?? Math.random
  const safeAttempt = Math.max(0, attempt)
  const ceiling = Math.min(cap, base * 2 ** safeAttempt)
  return Math.floor(random() * ceiling)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd front && npx vitest run app/app/trade/_lib/tape/backoff.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_lib/tape/backoff.ts front/app/app/trade/_lib/tape/backoff.test.ts
git commit -m "Add full-jitter exponential backoff helper for tape reconnection"
```

---

## Task 2: Feed merge reducer (pure)

**Files:**
- Create: `front/app/app/trade/_lib/tape/feed.ts`
- Test: `front/app/app/trade/_lib/tape/feed.test.ts`

- [ ] **Step 1: Write the failing test**

Create `front/app/app/trade/_lib/tape/feed.test.ts`:

```ts
import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import {
  AuctionEventSchema,
  AuctionSummarySchema,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  addLive,
  auctionEventToSummary,
  emptyFeed,
  mergeHistory,
  selectAuctions,
  MAX_RETAINED_AUCTIONS,
} from './feed'

function summary(id: string, ts: bigint) {
  return create(AuctionSummarySchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '3000.5',
    matchedVolume: '2.5',
    matchCount: 3,
    timestampUnix: ts,
  })
}

function event(id: string, ts: bigint) {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '3000.5',
    matchedVolume: '2.5',
    matchCount: 3,
    timestampUnix: ts,
  })
}

describe('feed reducer', () => {
  it('auctionEventToSummary preserves string decimals and the bigint timestamp', () => {
    const s = auctionEventToSummary(event('a1', 5n))
    expect(s.auctionId).toBe('a1')
    expect(s.clearingPrice).toBe('3000.5')
    expect(s.matchedVolume).toBe('2.5')
    expect(s.matchCount).toBe(3)
    expect(s.timestampUnix).toBe(5n)
  })

  it('mergeHistory dedups by auctionId (latest wins)', () => {
    let state = emptyFeed()
    state = mergeHistory(state, [summary('a1', 1n), summary('a2', 2n)])
    state = mergeHistory(state, [summary('a1', 9n)])
    const out = selectAuctions(state, 50)
    expect(out.map((a) => a.auctionId)).toEqual(['a1', 'a2'])
    expect(out[0].timestampUnix).toBe(9n)
  })

  it('addLive upserts and selectAuctions sorts newest-first', () => {
    let state = emptyFeed()
    state = mergeHistory(state, [summary('a1', 1n)])
    state = addLive(state, event('a2', 5n))
    state = addLive(state, event('a1', 1n)) // dup id, no growth
    const out = selectAuctions(state, 50)
    expect(out.map((a) => a.auctionId)).toEqual(['a2', 'a1'])
  })

  it('selectAuctions slices to the limit', () => {
    let state = emptyFeed()
    for (let i = 0; i < 10; i++) state = addLive(state, event(`a${i}`, BigInt(i)))
    expect(selectAuctions(state, 3)).toHaveLength(3)
  })

  it('prunes to MAX_RETAINED_AUCTIONS keeping the newest', () => {
    let state = emptyFeed()
    for (let i = 0; i < MAX_RETAINED_AUCTIONS + 50; i++) {
      state = addLive(state, event(`a${i}`, BigInt(i)))
    }
    const out = selectAuctions(state, MAX_RETAINED_AUCTIONS + 100)
    expect(out).toHaveLength(MAX_RETAINED_AUCTIONS)
    expect(out[0].auctionId).toBe(`a${MAX_RETAINED_AUCTIONS + 49}`) // newest
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd front && npx vitest run app/app/trade/_lib/tape/feed.test.ts`
Expected: FAIL — `Failed to resolve import "./feed"`.

- [ ] **Step 3: Write minimal implementation**

Create `front/app/app/trade/_lib/tape/feed.ts`:

```ts
// Pure merge layer for the tape: one keyed store fed by BOTH the REST history
// poll and the live SSE stream. Dedup is by auctionId, so an event already in
// history (or vice-versa) is a no-op. State is immutable (new Map per action)
// so React detects the change.

import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type {
  AuctionEvent,
  AuctionSummary,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

// Bound memory for a long-lived live stream. A 5 s cadence fills 200 rows in
// ~17 min; older clears scroll out of any realistic viewport.
export const MAX_RETAINED_AUCTIONS = 200

export interface FeedState {
  readonly byId: ReadonlyMap<string, AuctionSummary>
}

export function emptyFeed(): FeedState {
  return { byId: new Map() }
}

export function auctionEventToSummary(event: AuctionEvent): AuctionSummary {
  return create(AuctionSummarySchema, {
    auctionId: event.auctionId,
    pair: event.pair,
    clearingPrice: event.clearingPrice,
    matchedVolume: event.matchedVolume,
    matchCount: event.matchCount,
    timestampUnix: event.timestampUnix,
  })
}

// Newest first; stable tiebreak on id so equal-timestamp rows don't jitter.
function cmpDesc(a: AuctionSummary, b: AuctionSummary): number {
  if (a.timestampUnix === b.timestampUnix) {
    return a.auctionId < b.auctionId ? 1 : a.auctionId > b.auctionId ? -1 : 0
  }
  return a.timestampUnix > b.timestampUnix ? -1 : 1
}

function prune(map: Map<string, AuctionSummary>): Map<string, AuctionSummary> {
  if (map.size <= MAX_RETAINED_AUCTIONS) return map
  const newest = [...map.values()].sort(cmpDesc).slice(0, MAX_RETAINED_AUCTIONS)
  return new Map(newest.map((a) => [a.auctionId, a]))
}

export function mergeHistory(
  state: FeedState,
  summaries: readonly AuctionSummary[]
): FeedState {
  if (summaries.length === 0) return state
  const next = new Map(state.byId)
  for (const s of summaries) next.set(s.auctionId, s)
  return { byId: prune(next) }
}

export function addLive(state: FeedState, event: AuctionEvent): FeedState {
  const summary = auctionEventToSummary(event)
  const next = new Map(state.byId)
  next.set(summary.auctionId, summary)
  return { byId: prune(next) }
}

export function selectAuctions(state: FeedState, limit: number): AuctionSummary[] {
  const sorted = [...state.byId.values()].sort(cmpDesc)
  return limit > 0 ? sorted.slice(0, limit) : sorted
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd front && npx vitest run app/app/trade/_lib/tape/feed.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_lib/tape/feed.ts front/app/app/trade/_lib/tape/feed.test.ts
git commit -m "Add tape feed merge reducer for history + live auction events"
```

---

## Task 3: SDK SSE transport (`RestClient.streamAuctions`)

**Files:**
- Modify: `front/lib/sdk/client.ts` (the `streamAuctions` body + add private SSE helpers + import `AuctionEventSchema`)
- Modify: `front/lib/sdk/client.test.ts` (replace the UNIMPLEMENTED test)

- [ ] **Step 1: Write the failing tests**

In `front/lib/sdk/client.test.ts`, **replace** the entire existing block:

```ts
describe('RestClient.streamAuctions', () => {
  it('throws UNIMPLEMENTED — SSE bridge is not wired yet', async () => {
    // ...existing body...
  })
})
```

with:

```ts
function sseResponse(chunks: string[], init: ResponseInit = {}): Response {
  const encoder = new TextEncoder()
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const c of chunks) controller.enqueue(encoder.encode(c))
      controller.close()
    },
  })
  return new Response(stream, {
    status: 200,
    headers: { 'content-type': 'text/event-stream' },
    ...init,
  })
}

const AUCTION_FRAME =
  'event: auction\n' +
  'data: {"auctionId":"a1","pair":"ETH/USDC","clearingPrice":"3000.5",' +
  '"matchedVolume":"2.5","matchCount":3,"timestampUnix":"1717200000"}\n\n'

async function drain(iter: AsyncIterable<unknown>): Promise<unknown[]> {
  const out: unknown[] = []
  for await (const x of iter) out.push(x)
  return out
}

describe('RestClient.streamAuctions', () => {
  it('yields AuctionEvents parsed from SSE auction frames (decimals stay strings)', async () => {
    const { fetch, calls } = captureFetch(sseResponse([AUCTION_FRAME]))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const events = (await drain(
      client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }))
    )) as Array<{
      auctionId: string
      clearingPrice: string
      matchedVolume: string
      matchCount: number
      timestampUnix: bigint
    }>
    expect(events).toHaveLength(1)
    expect(events[0].auctionId).toBe('a1')
    expect(events[0].clearingPrice).toBe('3000.5')
    expect(events[0].matchedVolume).toBe('2.5')
    expect(events[0].matchCount).toBe(3)
    expect(events[0].timestampUnix).toBe(1717200000n)
    expect(calls[0].url).toBe(`${BASE}/v1/auctions/stream?pair=ETH%2FUSDC`)
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers.accept).toBe('text/event-stream')
    expect(headers['x-api-key']).toBe(KEY)
  })

  it('omits the query string when no pair is given', async () => {
    const { fetch, calls } = captureFetch(sseResponse([AUCTION_FRAME]))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await drain(client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' })))
    expect(calls[0].url).toBe(`${BASE}/v1/auctions/stream`)
  })

  it('ignores keep-alive comments and unknown event types', async () => {
    const chunks = [': keep-alive\n\n', 'event: ping\ndata: nope\n\n', AUCTION_FRAME]
    const { fetch } = captureFetch(sseResponse(chunks))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const events = await drain(
      client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' }))
    )
    expect(events).toHaveLength(1)
  })

  it('reassembles a frame split across chunks', async () => {
    const chunks = [
      'event: auction\ndata: {"auctionId":"a1"',
      ',"pair":"ETH/USDC","clearingPrice":"1","matchedVolume":"1","matchCount":0,"timestampUnix":"5"}\n\n',
    ]
    const { fetch } = captureFetch(sseResponse(chunks))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const events = (await drain(
      client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' }))
    )) as Array<{ auctionId: string }>
    expect(events).toHaveLength(1)
    expect(events[0].auctionId).toBe('a1')
  })

  it('throws DATA_LOSS on a lagged error frame', async () => {
    const { fetch } = captureFetch(sseResponse(['event: error\ndata: {"lagged":7}\n\n']))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      drain(client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' })))
    ).rejects.toMatchObject({
      name: 'DarkPoolError',
      code: DARK_POOL_ERROR_CODES.DATA_LOSS,
    })
  })

  it('maps an HTTP error response to a DarkPoolError', async () => {
    const { fetch } = captureFetch(new Response('', { status: 401 }))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      drain(client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' })))
    ).rejects.toMatchObject({ code: DARK_POOL_ERROR_CODES.UNAUTHENTICATED, httpStatus: 401 })
  })

  it('maps a fetch rejection to UNAVAILABLE', async () => {
    const fetch = vi.fn(async () => {
      throw new TypeError('connection refused')
    }) as unknown as typeof globalThis.fetch
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      drain(client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' })))
    ).rejects.toMatchObject({ code: DARK_POOL_ERROR_CODES.UNAVAILABLE })
  })

  it('terminates cleanly when the abort signal fires', async () => {
    const controller = new AbortController()
    const neverEnds = new ReadableStream<Uint8Array>({ start() {} })
    const fetch = vi.fn(
      async () => new Response(neverEnds, { status: 200 })
    ) as unknown as typeof globalThis.fetch
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const iter = client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: '' }), {
      signal: controller.signal,
    })
    let done = false
    const drained = (async () => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      for await (const _ of iter) {
        // unreachable
      }
      done = true
    })()
    await Promise.resolve()
    controller.abort()
    await drained
    expect(done).toBe(true)
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd front && npx vitest run lib/sdk/client.test.ts -t streamAuctions`
Expected: FAIL — current `streamAuctions` throws UNIMPLEMENTED for every case.

- [ ] **Step 3: Add the `AuctionEventSchema` import**

In `front/lib/sdk/client.ts`, add `AuctionEventSchema` to the existing value-import block from `./proto/darkpool/v1/darkpool_pb.js` (the block that currently imports `CancelOrderResponseSchema`, `GetAuctionHistoryResponseSchema`, …, `Side`):

```ts
import {
  AuctionEventSchema,
  CancelOrderResponseSchema,
  GetAuctionHistoryResponseSchema,
  GetOrderBookResponseSchema,
  GetOrderResponseSchema,
  OrderInfoSchema,
  PlaceOrderRequestSchema,
  PlaceOrderResponseSchema,
  Side,
} from './proto/darkpool/v1/darkpool_pb.js'
```

- [ ] **Step 4: Implement the `streamAuctions` body**

In `front/lib/sdk/client.ts`, **replace** the current `RestClient.streamAuctions` method (the `// eslint-disable-next-line require-yield` + `async *streamAuctions(...) { ... throw new DarkPoolError(UNIMPLEMENTED ...) }`) with:

```ts
  async *streamAuctions(
    req: StreamAuctionsRequest,
    opts?: StreamOptions
  ): AsyncIterable<AuctionEvent> {
    const signal = opts?.signal
    if (signal?.aborted) return

    const search = new URLSearchParams()
    if (req.pair) search.set('pair', req.pair)
    const qs = search.toString()
    const url = `${this.baseUrl}/v1/auctions/stream${qs ? `?${qs}` : ''}`

    let response: Response
    try {
      response = await this.fetchImpl(url, {
        method: 'GET',
        headers: { accept: 'text/event-stream', 'x-api-key': this.apiKey },
        signal,
      })
    } catch (cause) {
      if (signal?.aborted) return
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.UNAVAILABLE,
        `Network error contacting ${url}: ${(cause as Error)?.message ?? cause}`,
        { cause }
      )
    }

    if (!response.ok) throw await parseErrorResponse(response)
    if (!response.body) {
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.UNAVAILABLE,
        `Auction stream from ${url} returned no body`
      )
    }

    const reader = response.body.getReader()
    const onAbort = () => {
      void reader.cancel()
    }
    signal?.addEventListener('abort', onAbort, { once: true })

    const decoder = new TextDecoder()
    let buffer = ''
    try {
      while (true) {
        let chunk
        try {
          chunk = await reader.read()
        } catch (cause) {
          if (signal?.aborted) return
          throw new DarkPoolError(
            DARK_POOL_ERROR_CODES.UNAVAILABLE,
            `Auction stream from ${url} dropped: ${(cause as Error)?.message ?? cause}`,
            { cause }
          )
        }
        if (chunk.done) return
        buffer += decoder.decode(chunk.value, { stream: true })

        let split = nextSseFrame(buffer)
        while (split !== null) {
          const frame = parseSseFrame(split.frame)
          buffer = split.rest
          if (frame !== null) {
            if (frame.event === 'error') {
              throw new DarkPoolError(
                DARK_POOL_ERROR_CODES.DATA_LOSS,
                `Auction stream lagged: ${readLagged(frame.data) ?? 'unknown'} events dropped`
              )
            }
            if ((frame.event === 'auction' || frame.event === '') && frame.data !== '') {
              const json = JSON.parse(frame.data) as unknown
              yield fromJson(
                AuctionEventSchema,
                json as Parameters<typeof fromJson<typeof AuctionEventSchema>>[1]
              )
            }
            // any other event type is ignored
          }
          split = nextSseFrame(buffer)
        }
      }
    } finally {
      signal?.removeEventListener('abort', onAbort)
      reader.releaseLock()
    }
  }
```

- [ ] **Step 5: Add the private SSE parser helpers**

In `front/lib/sdk/client.ts`, add these module-private helpers immediately after the `RestClient` class (next to `parseErrorResponse`):

```ts
// ─── SSE frame parsing (private to streamAuctions) ─────────────────────────

interface SseFrame {
  event: string
  data: string
}

// Find the next event boundary (a blank line). Handles both LF and CRLF
// servers; returns the frame text and the remaining buffer, or null if no
// complete frame has arrived yet.
function nextSseFrame(buffer: string): { frame: string; rest: string } | null {
  const lf = buffer.indexOf('\n\n')
  const crlf = buffer.indexOf('\r\n\r\n')
  let idx = -1
  let len = 0
  if (lf !== -1 && (crlf === -1 || lf < crlf)) {
    idx = lf
    len = 2
  } else if (crlf !== -1) {
    idx = crlf
    len = 4
  }
  if (idx === -1) return null
  return { frame: buffer.slice(0, idx), rest: buffer.slice(idx + len) }
}

// Parse one frame into its `event` (last wins, '' if absent) and `data`
// (multiple data: lines joined by \n, per the SSE spec). Comment (`:`) lines
// and other fields (id, retry) are ignored.
function parseSseFrame(frame: string): SseFrame | null {
  let event = ''
  const dataLines: string[] = []
  for (let line of frame.split('\n')) {
    if (line.endsWith('\r')) line = line.slice(0, -1)
    if (line === '' || line.startsWith(':')) continue
    const colon = line.indexOf(':')
    const field = colon === -1 ? line : line.slice(0, colon)
    let value = colon === -1 ? '' : line.slice(colon + 1)
    if (value.startsWith(' ')) value = value.slice(1)
    if (field === 'event') event = value
    else if (field === 'data') dataLines.push(value)
  }
  if (event === '' && dataLines.length === 0) return null
  return { event, data: dataLines.join('\n') }
}

function readLagged(data: string): number | null {
  try {
    const parsed = JSON.parse(data) as { lagged?: unknown }
    return typeof parsed.lagged === 'number' ? parsed.lagged : null
  } catch {
    return null
  }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd front && npx vitest run lib/sdk/client.test.ts`
Expected: PASS (all streamAuctions cases + the untouched suite).

- [ ] **Step 7: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/lib/sdk/client.ts front/lib/sdk/client.test.ts
git commit -m "Wire RestClient.streamAuctions to the C3 SSE bridge"
```

---

## Task 4: Conditional history polling

**Files:**
- Modify: `front/app/app/trade/_hooks/tape/useAuctionHistory.ts`
- Create: `front/app/app/trade/_hooks/tape/useAuctionHistory.test.ts`

- [ ] **Step 1: Write the failing test**

Create `front/app/app/trade/_hooks/tape/useAuctionHistory.test.ts`:

```ts
// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

import { useAuctionHistory } from './useAuctionHistory'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import type { DarkPoolClient } from '@/lib/sdk/client'
import {
  create,
} from '@bufbuild/protobuf'
import { GetAuctionHistoryResponseSchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function clientWithSpy() {
  const getAuctionHistory = vi.fn(async () =>
    create(GetAuctionHistoryResponseSchema, { auctions: [] })
  )
  const client = { getAuctionHistory } as unknown as DarkPoolClient
  return { client, getAuctionHistory }
}

function makeWrapper(client: DarkPoolClient) {
  return function Wrapper({ children }: { children: React.ReactNode }): JSX.Element {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    return (
      <QueryClientProvider client={qc}>
        <DarkPoolClientProvider client={client}>{children}</DarkPoolClientProvider>
      </QueryClientProvider>
    )
  }
}

describe('useAuctionHistory polling toggle', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('does not poll again when refetchIntervalMs is false', async () => {
    const { client, getAuctionHistory } = clientWithSpy()
    renderHook(() => useAuctionHistory({ refetchIntervalMs: false }), {
      wrapper: makeWrapper(client),
    })
    await vi.advanceTimersByTimeAsync(0)
    await waitFor(() => expect(getAuctionHistory).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(10_000)
    expect(getAuctionHistory).toHaveBeenCalledTimes(1)
  })

  it('polls on the interval when given a number', async () => {
    const { client, getAuctionHistory } = clientWithSpy()
    renderHook(() => useAuctionHistory({ refetchIntervalMs: 1000 }), {
      wrapper: makeWrapper(client),
    })
    await vi.advanceTimersByTimeAsync(0)
    await waitFor(() => expect(getAuctionHistory).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(2500)
    expect(getAuctionHistory.mock.calls.length).toBeGreaterThanOrEqual(3)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionHistory.test.ts`
Expected: FAIL — `refetchIntervalMs: false` is rejected by the current `number`-only type (TS error) / polling not disabled.

- [ ] **Step 3: Widen the option type**

In `front/app/app/trade/_hooks/tape/useAuctionHistory.ts`, change the option field and keep the default. Replace:

```ts
  /** Override polling cadence — primarily for Storybook + tests. */
  refetchIntervalMs?: number
```

with:

```ts
  /**
   * Override polling cadence — primarily for Storybook + tests. Pass `false`
   * to disable polling entirely (used by useAuctionFeed while the live SSE
   * stream is connected).
   */
  refetchIntervalMs?: number | false
```

The body already reads `const refetchInterval = opts.refetchIntervalMs ?? AUCTION_HISTORY_POLL_MS` — `??` leaves a `false` intact (only null/undefined fall back), and TanStack Query accepts `refetchInterval: number | false`. No further body change is needed.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionHistory.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_hooks/tape/useAuctionHistory.ts front/app/app/trade/_hooks/tape/useAuctionHistory.test.ts
git commit -m "Allow useAuctionHistory polling to be disabled (refetchIntervalMs: false)"
```

---

## Task 5: Reconnect state machine (`useAuctionStream`)

**Files:**
- Create: `front/app/app/trade/_hooks/tape/useAuctionStream.ts`
- Test: `front/app/app/trade/_hooks/tape/useAuctionStream.test.tsx`

- [ ] **Step 1: Write the failing tests**

Create `front/app/app/trade/_hooks/tape/useAuctionStream.test.tsx`:

```tsx
// @vitest-environment jsdom
import { create } from '@bufbuild/protobuf'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

import { useAuctionStream } from './useAuctionStream'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import { DARK_POOL_ERROR_CODES, DarkPoolError, type DarkPoolClient } from '@/lib/sdk/client'
import {
  AuctionEventSchema,
  type AuctionEvent,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function ev(id: string): AuctionEvent {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '1',
    matchedVolume: '1',
    matchCount: 0,
    timestampUnix: 1n,
  })
}

type StreamFn = (signal: AbortSignal | undefined) => AsyncIterable<AuctionEvent>

// A DarkPoolClient whose streamAuctions plays the next scripted generator on
// each (re)connect, latching on the last one once the script is exhausted.
function scriptClient(streams: StreamFn[]): DarkPoolClient {
  let i = 0
  return {
    streamAuctions: (_req: unknown, opts?: { signal?: AbortSignal }) =>
      streams[Math.min(i++, streams.length - 1)](opts?.signal),
  } as unknown as DarkPoolClient
}

function makeWrapper(client: DarkPoolClient) {
  return function Wrapper({ children }: { children: React.ReactNode }): JSX.Element {
    return <DarkPoolClientProvider client={client}>{children}</DarkPoolClientProvider>
  }
}

// Yields the given events then blocks until the abort signal fires (a healthy,
// idle live connection).
async function* liveThenBlock(events: AuctionEvent[], signal?: AbortSignal) {
  for (const e of events) yield e
  await new Promise<void>((resolve) => {
    if (signal?.aborted) return resolve()
    signal?.addEventListener('abort', () => resolve(), { once: true })
  })
}

const ZERO_BACKOFF = { random: () => 0 }

describe('useAuctionStream', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('goes connecting → live and forwards events', async () => {
    const onEvent = vi.fn()
    const client = scriptClient([(signal) => liveThenBlock([ev('a1')], signal)])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(result.current.status).toBe('live'))
    expect(onEvent).toHaveBeenCalledTimes(1)
    expect(onEvent.mock.calls[0][0].auctionId).toBe('a1')
  })

  it('degrades then reconnects after a graceful end', async () => {
    const onEvent = vi.fn()
    // first connection: one event then ends; second: blocks live
    const client = scriptClient([
      async function* () {
        yield ev('a1')
      },
      (signal) => liveThenBlock([ev('a2')], signal),
    ])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(1))
    // graceful end → degraded, schedule(0) → reconnect
    await vi.advanceTimersByTimeAsync(5)
    await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(2))
    expect(result.current.status).toBe('live')
  })

  it('calls onLag and reconnects immediately on DATA_LOSS', async () => {
    const onLag = vi.fn()
    const onEvent = vi.fn()
    const client = scriptClient([
      async function* () {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.DATA_LOSS, 'lagged')
      },
      (signal) => liveThenBlock([ev('a2')], signal),
    ])
    renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, onLag, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(onLag).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(5)
    await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(1))
  })

  it('stops retrying on a terminal auth error', async () => {
    const onEvent = vi.fn()
    let connects = 0
    const client = scriptClient([
      async function* () {
        connects++
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAUTHENTICATED, 'no key')
      },
    ])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(result.current.status).toBe('degraded'))
    await vi.advanceTimersByTimeAsync(60_000)
    expect(connects).toBe(1) // no reconnect attempts
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionStream.test.tsx`
Expected: FAIL — `Failed to resolve import "./useAuctionStream"`.

- [ ] **Step 3: Write the implementation**

Create `front/app/app/trade/_hooks/tape/useAuctionStream.ts`:

```ts
'use client'

import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { useDarkPoolClient } from '@/lib/sdk/provider'
import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/sdk/client'
import {
  StreamAuctionsRequestSchema,
  type AuctionEvent,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { backoffDelay, type BackoffOptions } from '../../_lib/tape/backoff'

export type StreamConnectionStatus = 'connecting' | 'live' | 'degraded'

export interface UseAuctionStreamOptions {
  pair: string
  onEvent: (event: AuctionEvent) => void
  /** Called when the server reports it dropped events (broadcast lag). */
  onLag?: () => void
  /** Disable the stream (pure-polling mode); defaults to true. */
  enabled?: boolean
  /** Backoff tuning; defaults to full-jitter base 1 s / cap 30 s. */
  backoff?: BackoffOptions
}

// Auth / not-found / unimplemented mean reconnecting will not help — stop and
// let the REST poll surface the same error to the user.
const TERMINAL_CODES: ReadonlySet<number> = new Set([
  DARK_POOL_ERROR_CODES.UNAUTHENTICATED,
  DARK_POOL_ERROR_CODES.PERMISSION_DENIED,
  DARK_POOL_ERROR_CODES.NOT_FOUND,
  DARK_POOL_ERROR_CODES.UNIMPLEMENTED,
])

export function useAuctionStream(
  opts: UseAuctionStreamOptions
): { status: StreamConnectionStatus } {
  const { pair, enabled = true } = opts
  const client = useDarkPoolClient()
  const [status, setStatus] = React.useState<StreamConnectionStatus>('connecting')

  // Latest callbacks/config via refs so the effect doesn't re-subscribe (and
  // tear down the live connection) on every parent render.
  const onEventRef = React.useRef(opts.onEvent)
  const onLagRef = React.useRef(opts.onLag)
  const backoffRef = React.useRef(opts.backoff)
  onEventRef.current = opts.onEvent
  onLagRef.current = opts.onLag
  backoffRef.current = opts.backoff

  React.useEffect(() => {
    if (!enabled) return

    const abort = new AbortController()
    let attempt = 0
    let timer: ReturnType<typeof setTimeout> | null = null
    let stopped = false

    const schedule = (delayMs: number) => {
      timer = setTimeout(() => {
        void run()
      }, delayMs)
    }

    const run = async () => {
      if (stopped || abort.signal.aborted) return
      setStatus((s) => (s === 'live' ? s : 'connecting'))
      try {
        const stream = client.streamAuctions(
          create(StreamAuctionsRequestSchema, { pair }),
          { signal: abort.signal }
        )
        for await (const event of stream) {
          if (stopped) return
          attempt = 0
          setStatus('live')
          onEventRef.current(event)
        }
        // Graceful end (server/proxy closed) → degrade + backoff reconnect.
        if (stopped || abort.signal.aborted) return
        setStatus('degraded')
        schedule(backoffDelay(attempt++, backoffRef.current))
      } catch (err) {
        if (stopped || abort.signal.aborted) return
        if (
          err instanceof DarkPoolError &&
          err.code === DARK_POOL_ERROR_CODES.DATA_LOSS
        ) {
          onLagRef.current?.()
          attempt = 0
          schedule(0) // link is healthy; reconnect immediately to catch up
          return
        }
        setStatus('degraded')
        if (err instanceof DarkPoolError && TERMINAL_CODES.has(err.code)) {
          return // stop retrying
        }
        schedule(backoffDelay(attempt++, backoffRef.current))
      }
    }

    void run()

    return () => {
      stopped = true
      if (timer) clearTimeout(timer)
      abort.abort()
    }
  }, [client, pair, enabled])

  return { status }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionStream.test.tsx`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_hooks/tape/useAuctionStream.ts front/app/app/trade/_hooks/tape/useAuctionStream.test.tsx
git commit -m "Add useAuctionStream reconnect state machine over the SSE stream"
```

---

## Task 6: Feed composition (`useAuctionFeed`)

**Files:**
- Create: `front/app/app/trade/_hooks/tape/useAuctionFeed.ts`
- Test: `front/app/app/trade/_hooks/tape/useAuctionFeed.test.tsx`

- [ ] **Step 1: Write the failing tests**

Create `front/app/app/trade/_hooks/tape/useAuctionFeed.test.tsx`:

```tsx
// @vitest-environment jsdom
import { create } from '@bufbuild/protobuf'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

import { useAuctionFeed } from './useAuctionFeed'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import type { DarkPoolClient } from '@/lib/sdk/client'
import {
  AuctionEventSchema,
  AuctionSummarySchema,
  GetAuctionHistoryResponseSchema,
  type AuctionEvent,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function ev(id: string, ts: bigint): AuctionEvent {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '1',
    matchedVolume: '1',
    matchCount: 0,
    timestampUnix: ts,
  })
}

function historyResponse(ids: Array<[string, bigint]>) {
  return create(GetAuctionHistoryResponseSchema, {
    auctions: ids.map(([id, ts]) =>
      create(AuctionSummarySchema, {
        auctionId: id,
        pair: 'ETH/USDC',
        clearingPrice: '1',
        matchedVolume: '1',
        matchCount: 0,
        timestampUnix: ts,
      })
    ),
  })
}

interface FakeOpts {
  history?: ReturnType<typeof historyResponse>
  stream?: (signal?: AbortSignal) => AsyncIterable<AuctionEvent>
}

function fakeClient(opts: FakeOpts) {
  const getAuctionHistory = vi.fn(async () => opts.history ?? historyResponse([]))
  const streamAuctions =
    opts.stream ??
    (async function* () {
      await new Promise(() => {}) // block forever
    })
  const client = { getAuctionHistory, streamAuctions } as unknown as DarkPoolClient
  return { client, getAuctionHistory }
}

function makeWrapper(client: DarkPoolClient) {
  return function Wrapper({ children }: { children: React.ReactNode }): JSX.Element {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    return (
      <QueryClientProvider client={qc}>
        <DarkPoolClientProvider client={client}>{children}</DarkPoolClientProvider>
      </QueryClientProvider>
    )
  }
}

describe('useAuctionFeed', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('merges history backfill with live stream events, newest first', async () => {
    async function* stream(signal?: AbortSignal) {
      yield ev('live2', 20n)
      await new Promise<void>((r) => signal?.addEventListener('abort', () => r(), { once: true }))
    }
    const { client } = fakeClient({ history: historyResponse([['hist1', 10n]]), stream })
    const { result } = renderHook(() => useAuctionFeed({ limit: 50 }), {
      wrapper: makeWrapper(client),
    })
    await vi.advanceTimersByTimeAsync(0)
    await waitFor(() =>
      expect(result.current.auctions.map((a) => a.auctionId)).toEqual(['live2', 'hist1'])
    )
    expect(result.current.status).toBe('live')
  })

  it('stops polling history once the stream is live', async () => {
    async function* stream(signal?: AbortSignal) {
      yield ev('live1', 20n)
      await new Promise<void>((r) => signal?.addEventListener('abort', () => r(), { once: true }))
    }
    const { client, getAuctionHistory } = fakeClient({ stream })
    renderHook(() => useAuctionFeed({ limit: 50, refetchIntervalMs: 1000 }), {
      wrapper: makeWrapper(client),
    })
    await waitFor(() => expect(getAuctionHistory).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(5000)
    expect(getAuctionHistory).toHaveBeenCalledTimes(1) // poll disabled while live
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionFeed.test.tsx`
Expected: FAIL — `Failed to resolve import "./useAuctionFeed"`.

- [ ] **Step 3: Write the implementation**

Create `front/app/app/trade/_hooks/tape/useAuctionFeed.ts`:

```ts
'use client'

import * as React from 'react'

import { DEFAULT_PAIR } from '@/lib/sdk/mocks/factories'
import type {
  AuctionEvent,
  AuctionSummary,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  addLive,
  emptyFeed,
  mergeHistory,
  selectAuctions,
  type FeedState,
} from '../../_lib/tape/feed'
import {
  AUCTION_HISTORY_POLL_MS,
  DEFAULT_AUCTION_HISTORY_LIMIT,
  useAuctionHistory,
} from './useAuctionHistory'
import { useAuctionStream, type StreamConnectionStatus } from './useAuctionStream'

export interface UseAuctionFeedOptions {
  pair?: string
  limit?: number
  /** Storybook/test override for the degraded poll cadence. */
  refetchIntervalMs?: number
}

export interface AuctionFeed {
  auctions: AuctionSummary[]
  status: StreamConnectionStatus
}

type FeedAction =
  | { type: 'history'; auctions: readonly AuctionSummary[] }
  | { type: 'live'; event: AuctionEvent }

function feedReducer(state: FeedState, action: FeedAction): FeedState {
  switch (action.type) {
    case 'history':
      return mergeHistory(state, action.auctions)
    case 'live':
      return addLive(state, action.event)
  }
}

/**
 * The single data source the Tape consumes. Tries the live SSE stream first;
 * while it is connected the REST history poll is disabled, and on any drop the
 * poll resumes (seamless degrade) while the stream reconnects with backoff. A
 * server-reported lag triggers a one-shot history refetch to backfill the gap.
 */
export function useAuctionFeed(opts: UseAuctionFeedOptions = {}): AuctionFeed {
  const pair = opts.pair ?? DEFAULT_PAIR
  const limit = opts.limit ?? DEFAULT_AUCTION_HISTORY_LIMIT

  const [state, dispatch] = React.useReducer(feedReducer, undefined, emptyFeed)

  // onLag must call history.refetch, but history is created below; bounce
  // through a ref so the callback identity stays stable.
  const refetchRef = React.useRef<() => void>(() => {})
  const onEvent = React.useCallback(
    (event: AuctionEvent) => dispatch({ type: 'live', event }),
    []
  )
  const onLag = React.useCallback(() => refetchRef.current(), [])

  const { status } = useAuctionStream({ pair, onEvent, onLag })

  const pollInterval: number | false =
    status === 'live' ? false : opts.refetchIntervalMs ?? AUCTION_HISTORY_POLL_MS
  const history = useAuctionHistory({ pair, limit, refetchIntervalMs: pollInterval })
  refetchRef.current = () => {
    void history.refetch()
  }

  const historyData = history.data
  React.useEffect(() => {
    if (historyData) dispatch({ type: 'history', auctions: historyData.auctions })
  }, [historyData])

  const auctions = React.useMemo(() => selectAuctions(state, limit), [state, limit])
  return { auctions, status }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd front && npx vitest run app/app/trade/_hooks/tape/useAuctionFeed.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_hooks/tape/useAuctionFeed.ts front/app/app/trade/_hooks/tape/useAuctionFeed.test.tsx
git commit -m "Add useAuctionFeed composing live stream with degraded REST poll"
```

---

## Task 7: Status badge (`StreamStatus`)

**Files:**
- Create: `front/app/app/trade/_components/tape/StreamStatus.tsx`
- Test: `front/app/app/trade/_components/tape/StreamStatus.test.tsx`
- Create: `front/app/app/trade/_components/tape/StreamStatus.stories.tsx`

- [ ] **Step 1: Write the failing test**

Create `front/app/app/trade/_components/tape/StreamStatus.test.tsx`:

```tsx
// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { StreamStatus } from './StreamStatus'

describe('StreamStatus', () => {
  it('shows a blinking white pill labelled LIVE when live', () => {
    const { container } = render(<StreamStatus status="live" />)
    expect(screen.getByText('LIVE')).toBeTruthy()
    const pill = container.querySelector('span[aria-hidden="true"]')
    expect(pill?.className).toContain('bg-brand-fg')
    expect(pill?.className).toContain('animate-blink')
  })

  it('shows a static muted pill labelled DELAYED when degraded', () => {
    const { container } = render(<StreamStatus status="degraded" />)
    expect(screen.getByText('DELAYED')).toBeTruthy()
    const pill = container.querySelector('span[aria-hidden="true"]')
    expect(pill?.className).toContain('bg-brand-muted')
    expect(pill?.className).not.toContain('animate-blink')
  })

  it('labels the connecting state DELAYED too', () => {
    render(<StreamStatus status="connecting" />)
    expect(screen.getByText('DELAYED')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd front && npx vitest run app/app/trade/_components/tape/StreamStatus.test.tsx`
Expected: FAIL — `Failed to resolve import "./StreamStatus"`.

- [ ] **Step 3: Write the implementation**

Create `front/app/app/trade/_components/tape/StreamStatus.tsx`:

```tsx
import * as React from 'react'

import type { StreamConnectionStatus } from '../../_hooks/tape/useAuctionStream'

// Mirrors `my-orders/StatusPill` and DESIGN.md `status-pill-*`: a 6×6 square
// (h-1.5 w-1.5, borderRadius 0) paired with a mono body-sm label. Per
// DESIGN-INSPIRATIONS §"Accent budget per view", the /app/trade surface
// already spends its single lime accent on the auction Countdown — so "live"
// reads through shape + motion (white square, 1 Hz blink), NOT colour.
// "Delayed" is a static muted square.
const SQUARE_BASE = 'inline-block h-1.5 w-1.5 shrink-0 align-middle [border-radius:0px]'

const PILL: Record<StreamConnectionStatus, string> = {
  live: `${SQUARE_BASE} bg-brand-fg animate-blink motion-reduce:animate-none`,
  connecting: `${SQUARE_BASE} bg-brand-muted`,
  degraded: `${SQUARE_BASE} bg-brand-muted`,
}

const LABEL: Record<StreamConnectionStatus, string> = {
  live: 'LIVE',
  connecting: 'DELAYED',
  degraded: 'DELAYED',
}

const LABEL_COLOR: Record<StreamConnectionStatus, string> = {
  live: 'text-brand-fg',
  connecting: 'text-brand-muted',
  degraded: 'text-brand-muted',
}

export interface StreamStatusProps {
  status: StreamConnectionStatus
}

export function StreamStatus({ status }: StreamStatusProps): JSX.Element {
  return (
    <span className="inline-flex items-center gap-2 font-mono text-body-sm" aria-live="polite">
      <span className={PILL[status]} aria-hidden="true" />
      <span className={`uppercase tracking-label ${LABEL_COLOR[status]}`}>{LABEL[status]}</span>
    </span>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd front && npx vitest run app/app/trade/_components/tape/StreamStatus.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Add the Storybook story**

Create `front/app/app/trade/_components/tape/StreamStatus.stories.tsx`:

```tsx
import type { Meta, StoryObj } from '@storybook/react'

import { StreamStatus } from './StreamStatus'

const meta: Meta<typeof StreamStatus> = {
  title: 'Trade/Tape/StreamStatus',
  component: StreamStatus,
}
export default meta

type Story = StoryObj<typeof StreamStatus>

export const Live: Story = { args: { status: 'live' } }
export const Connecting: Story = { args: { status: 'connecting' } }
export const Degraded: Story = { args: { status: 'degraded' } }
```

- [ ] **Step 6: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_components/tape/StreamStatus.tsx front/app/app/trade/_components/tape/StreamStatus.test.tsx front/app/app/trade/_components/tape/StreamStatus.stories.tsx
git commit -m "Add StreamStatus LIVE/DELAYED badge for the tape"
```

---

## Task 8: Wire the feed and badge into the Tape

**Files:**
- Modify: `front/app/app/trade/_components/tape/Countdown.tsx`
- Modify: `front/app/app/trade/_components/tape/Tape.tsx`

- [ ] **Step 1: Add an optional `status` prop to Countdown**

The real `Countdown` is a centered bracketed bar (`[ NEXT AUCTION IN xx ]`)
that already uses the lime accent for the active state. Add the badge at its
right edge via absolute positioning so the centered label is undisturbed.

In `front/app/app/trade/_components/tape/Countdown.tsx`, add imports below the
existing `secondsToNextAuction` import:

```ts
import { StreamStatus } from './StreamStatus'
import type { StreamConnectionStatus } from '../../_hooks/tape/useAuctionStream'
```

Extend the props (keep the existing `intervalSeconds`):

```ts
export interface CountdownProps {
  latestAuctionUnixSeconds: bigint | null
  nowUnixSeconds: number
  /** Cadence of the auction tick. Mock store defaults to 5s. */
  intervalSeconds?: number
  /** Live-feed status; renders the LIVE/DELAYED badge at the bar's right edge. */
  status?: StreamConnectionStatus
}
```

Replace the function body (signature + `return`) with — note `relative` added
to the bar and the absolutely-positioned badge appended after `{label}`:

```tsx
export function Countdown({
  latestAuctionUnixSeconds,
  nowUnixSeconds,
  intervalSeconds = DEFAULT_INTERVAL_SECONDS,
  status,
}: CountdownProps): JSX.Element {
  const waiting = latestAuctionUnixSeconds === null

  const label = waiting
    ? '[ WAITING FOR FIRST AUCTION ]'
    : `[ NEXT AUCTION IN ${pad2(
        secondsToNextAuction(latestAuctionUnixSeconds, nowUnixSeconds, intervalSeconds)
      )} ]`

  return (
    <div
      aria-live="polite"
      aria-atomic="true"
      className={`relative flex h-9 items-center justify-center border-b border-brand-border bg-brand-bg px-4 font-mono text-label-lg uppercase tracking-label ${
        waiting ? 'text-brand-muted' : 'text-brand-accent'
      }`}
    >
      {label}
      {status ? (
        <span className="absolute right-3 top-1/2 -translate-y-1/2">
          <StreamStatus status={status} />
        </span>
      ) : null}
    </div>
  )
}
```

Leave the `pad2` helper at the bottom of the file unchanged.

- [ ] **Step 2: Switch Tape to `useAuctionFeed`**

In `front/app/app/trade/_components/tape/Tape.tsx`:

Replace the imports:

```ts
import { auctionsFromQuery } from '../../_lib/tape/auctions'
import { useAuctionHistory } from '../../_hooks/tape/useAuctionHistory'
```

with:

```ts
import { useAuctionFeed } from '../../_hooks/tape/useAuctionFeed'
```

Replace the body of `TapeContent` (the first three lines that build `query`,
`auctions`, `nowSeconds`) so it reads:

```tsx
export function TapeContent({ limit, refetchIntervalMs }: TapeProps = {}): JSX.Element {
  const { auctions, status } = useAuctionFeed({ limit, refetchIntervalMs })
  const nowSeconds = useNow()
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
```

Pass `status` to the Countdown:

```tsx
      <Countdown
        latestAuctionUnixSeconds={latestUnix}
        nowUnixSeconds={nowSeconds}
        status={status}
      />
```

(Leave `auctions.ts` / `auctionsFromQuery` and its test in place — it remains a
valid `_lib` export; only the Tape stops importing it.)

- [ ] **Step 3: Run the existing tape tests + type-check**

Run: `cd front && npx vitest run app/app/trade/_components/tape && npx tsc --noEmit`
Expected: PASS — existing `states.test.tsx`/`StreamStatus.test.tsx` pass and there are no type errors. If `tsc` flags an unused `auctionsFromQuery` import that was left in `Tape.tsx`, remove that import line.

- [ ] **Step 4: Commit**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git add front/app/app/trade/_components/tape/Countdown.tsx front/app/app/trade/_components/tape/Tape.tsx
git commit -m "Render the live auction feed and LIVE/DELAYED badge in the tape"
```

---

## Task 9: Full verification + PR

**Files:** none (verification only).

- [ ] **Step 1: Run the full front-end test suite**

Run: `cd front && npx vitest run`
Expected: PASS — all suites green, including the new tape + SDK tests.

- [ ] **Step 2: Lint + type-check**

Run: `cd front && npm run lint && npx tsc --noEmit`
Expected: no errors. Fix any `eslint` issues (e.g. the `require-yield` disable
comment on the old `streamAuctions` was removed with the body — confirm none
remains).

- [ ] **Step 3: Production build smoke test**

Run: `cd front && npm run build`
Expected: build succeeds (App Router compiles the trade route).

- [ ] **Step 4: Rebase on fresh main (catch a possible #148 landing)**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git fetch origin
git rebase origin/main
```

If #148 (SIWE) merged and refactored `client.ts` headers, resolve the trivial
conflict by routing the SSE request through its shared header builder instead
of the inline `{ accept, 'x-api-key' }` object. Re-run Step 1 after rebasing.

- [ ] **Step 5: Push and open the PR**

```bash
cd /home/mario/darkpool-wt/95-auction-streaming
git push -u origin feat/issue-95-auction-streaming
gh pr create --title "[I2.6] Auction streaming upgrade" --body "$(cat <<'EOF'
Closes #95

Upgrades the auction tape from REST polling (I2.5) to the C3 SSE bridge.

- `RestClient.streamAuctions` reads `GET /v1/auctions/stream` (auth-gated SSE)
  via fetch + ReadableStream, yielding `AuctionEvent`s; throws `DATA_LOSS` on a
  server lag frame.
- `useAuctionStream` owns reconnection (full-jitter exponential backoff, cap
  30 s) and terminal-error handling.
- `useAuctionFeed` merges live events with the I2.5 history poll, which is
  disabled while live and resumes on any drop (seamless degrade); a lag frame
  triggers a one-shot history backfill.
- The tape shows a LIVE/DELAYED status badge (DESIGN.md `status-pill-*`).

Works in both mock and real modes via `NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS`.
EOF
)"
```

- [ ] **Step 6: Confirm CI is green** on the PR; address any failures.

---

## Self-Review

**Spec coverage**

| Spec item | Task |
|---|---|
| SSE transport via fetch + ReadableStream | Task 3 |
| Numeric fields stay strings; `timestampUnix` bigint | Tasks 2, 3 (assertions) |
| `streamAuctions` returns an async iterable | Task 3 |
| `DATA_LOSS` on lag frame | Task 3 |
| Exponential backoff + jitter, cap 30 s | Task 1, Task 5 |
| Reconnect FSM, terminal vs retryable | Task 5 |
| Degrade to REST polling (I2.5) on drop | Tasks 4, 6 |
| History poll disabled while live | Tasks 4, 6 |
| Lag → one-shot history backfill | Task 6 (`onLag` → `refetch`) |
| LIVE/DELAYED badge (DESIGN tokens) | Tasks 7, 8 |
| Merge/dedup/cap of history + live | Task 2 |
| Works in mock + real modes (no env change) | inherent (provider/flag unchanged) |

**Placeholder scan:** none — every code/test step contains full content.

**Type consistency:** `StreamConnectionStatus` (Task 5) is imported by Tasks 6/7/8.
`FeedState`, `emptyFeed`, `mergeHistory`, `addLive`, `selectAuctions`,
`auctionEventToSummary` (Task 2) are used by Task 6. `backoffDelay` /
`BackoffOptions` (Task 1) are used by Task 5. `refetchIntervalMs: number | false`
(Task 4) is the type `useAuctionFeed` passes (Task 6). `AuctionEventSchema`
import (Task 3) matches `fromJson` usage. Header object `{ accept, 'x-api-key' }`
matches the existing `requestJson` convention.

**Scope:** all files are within `front/app/app/trade/_{components,hooks,lib}/tape/`
and `front/lib/sdk/client.ts` (+ its test). No interface/`StreamOptions` change;
`client.ts` gains one import + three private helpers beyond the method body.
