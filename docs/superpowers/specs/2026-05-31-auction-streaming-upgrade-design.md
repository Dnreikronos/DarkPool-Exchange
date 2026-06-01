# [I2.6 / #95] Auction streaming upgrade — Design

**Issue:** [#95](https://github.com/Dnreikronos/DarkPool-Exchange/issues/95) ·
**Epic:** [#62](https://github.com/Dnreikronos/DarkPool-Exchange/issues/62) ·
**Depends on:** C3 (#83, merged — SSE), I2.5 (#94/#151, merged — REST tape)

## Why

Upgrade the auction tape from 2 s REST polling (I2.5) to a live server-push
stream. Real-time clears feel materially better and cut backend load. The
stream must degrade gracefully back to polling whenever it drops, so the tape
never goes stale.

## What C3 actually shipped

C3 landed **SSE**, not gRPC-web. The relevant surface in
`crates/dp-api/src/rest.rs`:

- `GET /v1/auctions/stream` (optional `?pair=` filter), mounted on the
  **auth-gated** public router (`auth_axum_mw` wraps it via
  `router_with_middleware` / `router_with_admin`). Requests need `x-api-key`
  (or, once frontend #148 lands, a Bearer token).
- Emits named SSE events:
  - `event: auction`, `data:` = JSON `AuctionEventSseJson`
    `{ auctionId, pair, clearingPrice, matchedVolume, matchCount, timestampUnix }`.
    This is exactly the **proto3-JSON encoding of `AuctionEvent`**: int64
    `timestampUnix` is a string, int32 `matchCount` is a number, the decimal
    fields are strings.
  - `event: error`, `data:` = `{"lagged":N}` when the server-side broadcast
    buffer overflows (the client fell behind and N events were dropped).
  - `:`-prefixed keep-alive comments every 15 s.

`AuctionEvent` and `AuctionSummary` are structurally identical in the proto
(same six fields), so an event maps to a tape row by a plain field copy, and a
parsed SSE frame decodes straight via `fromJson(AuctionEventSchema, …)`.

## Decisions (confirmed with the issue owner)

1. **Reconnect + degrade live in the tape, not the SDK.**
   `RestClient.streamAuctions` is a thin transport: one SSE connection that
   yields `AuctionEvent`s and ends/throws on drop. The policy (reconnect,
   fall back to polling) lives in a tape hook, matching the rest of
   `RestClient` (every other method is a pure transport seam).
2. **Seamless degrade.** On drop, REST polling resumes *immediately* so rows
   keep updating, while the stream reconnects in the background; polling stops
   the instant the stream is live again. A `LIVE` / `DELAYED` badge reflects
   the mode.
3. **Backfill once on lag.** A `{"lagged":N}` frame is non-fatal: trigger a
   one-shot `getAuctionHistory` refetch to backfill the missed rows and
   reconnect immediately (the link is healthy — no backoff).

## Transport choice: fetch + ReadableStream, not `EventSource`

Native `EventSource` is ruled out on two independent grounds:

- The endpoint is auth-gated and `EventSource` cannot set request headers
  (`x-api-key` / `Authorization`).
- The issue mandates custom reconnection (exponential backoff + jitter, cap
  30 s); `EventSource` only honors the server `retry:` hint with no jitter or
  cap.

So `streamAuctions` opens the connection with `fetch`, reads `response.body`
via a `ReadableStreamDefaultReader`, decodes with `TextDecoder`, and parses SSE
frames by hand. This reuses the injectable `fetch` already on `RestClient`, so
tests drive it with a `Response` whose body is a `ReadableStream`.

## Architecture — three layers

```
RestClient.streamAuctions  ──>  useAuctionStream  ──>  useAuctionFeed  ──>  Tape
(SDK: 1 SSE connection)        (reconnect FSM)       (merge + degrade)    (rows + badge)
        │                            │                      │
   fetch+ReadableStream         backoff (_lib)        useAuctionHistory (I2.5 poll)
   yields AuctionEvent          full-jitter,cap 30s   = the REST fallback
```

### Layer 1 — SDK transport (`front/lib/sdk/client.ts`, body of `streamAuctions`)

```
async *streamAuctions(req, opts): AsyncIterable<AuctionEvent>
```

- Build URL `${baseUrl}/v1/auctions/stream`, append `?pair=` (URL-encoded)
  when `req.pair` is non-empty.
- `fetch` with `method: 'GET'`, headers `{ accept: 'text/event-stream',
  'x-api-key': this.apiKey }`, `signal: opts?.signal`. A `fetch` rejection →
  `DarkPoolError(UNAVAILABLE, …, { cause })` (mirrors `requestJson`).
- `!response.ok` → throw `await parseErrorResponse(response)` (401 →
  `UNAUTHENTICATED`, 403 → `PERMISSION_DENIED`, …). Lets the FSM distinguish
  terminal from retryable.
- `response.body == null` → `DarkPoolError(UNAVAILABLE, …)`.
- Read frames via a module-private parser:
  - Accumulate decoded text; split frames on a blank line, tolerating both
    `\n\n` and `\r\n\r\n`; buffer the trailing partial frame across chunks.
  - Per frame, parse `event:` and (possibly multi-line, `\n`-joined) `data:`
    fields; ignore `:` comment lines, `id:`, `retry:`, and unknown events.
  - `event === 'auction'` (or unspecified, defensively) →
    `yield fromJson(AuctionEventSchema, JSON.parse(data))`.
  - `event === 'error'` with `{ lagged }` → throw
    `DarkPoolError(DATA_LOSS, 'auction stream lagged: N events dropped')`.
- Reader `done` (graceful close) → return (generator ends).
- On `signal` abort or `finally`: cancel the reader and release the lock — no
  leaked connection. Already-aborted signal → return immediately.

Constraints honored: signature and `StreamOptions` are **unchanged**; the only
additions to `client.ts` beyond the method body are one import
(`AuctionEventSchema`) and one private parser function. No new exports → minimal
collision surface with #148, which owns the file's header/auth structure. If
#148 merges first and extracts a shared header builder, the rebase is a
one-line swap of the inline `x-api-key` header for that helper.

### Layer 2 — reconnect FSM (`_hooks/tape/useAuctionStream.ts`, new)

Drives the connection lifecycle against `useDarkPoolClient()`. Inputs:
`{ pair, onEvent, onLag, enabled?, backoff? }`. Output: `{ status }` where
`status ∈ 'connecting' | 'live' | 'degraded'`.

Loop (guarded by a per-mount `AbortController`):

- Open `client.streamAuctions({ pair }, { signal })`; mark `connecting`.
- For each event: `status = 'live'`, **reset the backoff attempt counter**,
  call `onEvent(event)`.
- Stream ends (done) or throws `UNAVAILABLE` / unknown → `status = 'degraded'`,
  wait `backoff.next(attempt++)`, reconnect.
- Throws `DATA_LOSS` (lag) → call `onLag()`, reconnect **immediately** (reset
  attempt; the link is fine).
- Throws terminal auth (`UNAUTHENTICATED` / `PERMISSION_DENIED` / `NOT_FOUND`
  / `UNIMPLEMENTED`) → `status = 'degraded'` and **stop** retrying the stream.
  Polling, which would also fail auth, surfaces the honest error to the user.
- Unmount / `enabled` flips false → `abort()`, cancel any pending backoff timer.

`enabled` lets Storybook/tests disable the stream to exercise pure-polling mode.

### Layer 3 — merge + degrade (`_hooks/tape/useAuctionFeed.ts`, new)

The single hook the Tape consumes. Inputs `{ pair, limit, refetchIntervalMs? }`,
output `{ auctions, status }`.

- `const stream = useAuctionStream({ pair, onEvent, onLag })`.
- `const history = useAuctionHistory({ pair, limit, refetchIntervalMs:
  stream.status === 'live' ? false : (refetchIntervalMs ?? POLL_MS) })`.
  This *is* the I2.5 fallback: it runs once on mount (initial backfill, since
  SSE only carries future events), polls while not-live (seamless degrade), and
  idles while live.
- `onLag = () => history.refetch()` (one-shot backfill).
- `onEvent = (e) => dispatch({ type: 'addLive', event: e })`.
- A `useEffect` upserts `history.data?.auctions` into the merged state whenever
  it changes.
- Merged state is held by a pure reducer (see `_lib/tape/feed.ts`).
- `refetchIntervalMs` passthrough keeps the existing Storybook/test override on
  `Tape` working.

### Pure helpers (`_lib/tape/`, new — unit-tested first)

- **`backoff.ts`** — full-jitter exponential backoff:
  `delay(attempt) = random() * min(cap, base * 2 ** attempt)`, with
  `base = 1000`, `cap = 30000`, and an injectable `random: () => number` for
  deterministic tests. `attempt = 0` → `[0, base)`; growth saturates at `cap`.
- **`feed.ts`** — pure merge reducer over `Map<auctionId, AuctionSummary>`:
  - `auctionEventToSummary(e)` → field copy into an `AuctionSummary`.
  - `mergeHistory(state, summaries)` and `addLive(state, event)` upsert by id.
  - Selector: values sorted by `timestampUnix` desc, capped to `max(4*limit,
    200)` to bound a long-lived live stream, then sliced to `limit` for render.
  - Dedup is by `auctionId`, so a live event already present in history is a
    no-op (and vice-versa).

### UI (`_components/tape/`)

- **`StreamStatus.tsx`** (new, + story): a `body-sm` badge mirroring the
  DESIGN.md `status-pill-*` tokens and the existing `my-orders/StatusPill.tsx`
  pattern (tape-scoped — different semantics, no cross-feature import). The
  `/app/trade` surface already spends its single lime accent on the auction
  `Countdown` (per DESIGN-INSPIRATIONS §"Accent budget per view"), so "live"
  reads through **shape + motion, not colour**:
  - `live` → 6×6 `bg-brand-fg` (white) square, `animate-blink
    motion-reduce:animate-none`, white `LIVE` label.
  - `connecting` / `degraded` → 6×6 `bg-brand-muted` static square, muted
    `DELAYED` label.
  - No lime here — the accent stays on the Countdown, keeping the panel to ≤1
    lime element.
- **`Tape.tsx`**: swap `useAuctionHistory` for `useAuctionFeed`; render
  `StreamStatus` in the header row (beside the Countdown). Rows, Countdown,
  Drawer, empty state, and the `QueryClientProvider` scoping are unchanged.

## File plan

All paths are inside the declared scope (`front/app/app/trade/_{components,
hooks,lib}/tape/` and `front/lib/sdk/client.ts`):

| File | Change |
|---|---|
| `front/lib/sdk/client.ts` | implement `streamAuctions` body + private SSE parser + `AuctionEventSchema` import |
| `front/lib/sdk/client.test.ts` | replace the UNIMPLEMENTED test with SSE tests |
| `_lib/tape/backoff.ts` + `.test.ts` | new — pure full-jitter backoff |
| `_lib/tape/feed.ts` + `.test.ts` | new — pure merge reducer + event→summary |
| `_hooks/tape/useAuctionStream.ts` + `.test.tsx` | new — reconnect FSM |
| `_hooks/tape/useAuctionFeed.ts` + `.test.tsx` | new — compose history + stream |
| `_hooks/tape/useAuctionHistory.ts` | allow `refetchIntervalMs: number \| false` |
| `_components/tape/StreamStatus.tsx` (+ story) | new — LIVE/DELAYED badge |
| `_components/tape/Tape.tsx` | use `useAuctionFeed`, render badge |

`client.test.ts` is edited by necessity — it is the test file for the method
this issue owns.

## Test plan (TDD, in implementation order)

1. **`backoff`** — `attempt 0 ∈ [0, base)`; monotonic ceiling growth; saturates
   at `cap` (30 s); jitter stays within `[0, ceiling)`; deterministic with an
   injected RNG.
2. **`feed`** — `mergeHistory` dedups by id; `addLive` upserts + dedups; sort is
   `timestampUnix` desc; output capped; `auctionEventToSummary` preserves
   string decimals and the bigint timestamp.
3. **`streamAuctions`** (in `client.test.ts`, via injected `fetch` returning a
   `ReadableStream`): yields parsed `AuctionEvent`s (decimals stay strings,
   `timestampUnix` is bigint); sends `accept: text/event-stream` + `x-api-key`;
   `?pair=` only when set; ignores keep-alive comments and unknown events;
   reassembles a frame split across chunks; throws `DATA_LOSS` on a lag frame;
   maps a 401 body to `UNAUTHENTICATED`; a `fetch` throw to `UNAVAILABLE`;
   aborts cleanly on `signal` with no hang.
4. **`useAuctionStream`** (fake timers + fake async-generator client):
   `connecting → live` on first event; `onEvent` per event; end → `degraded`
   + backoff-scheduled reconnect; `DATA_LOSS` → `onLag` + immediate reconnect;
   terminal auth → `degraded`, no further retries; unmount aborts.
5. **`useAuctionFeed`**: live events merge with history; history poll disabled
   while live; polling resumes on drop; lag triggers `history.refetch()`.
6. **`StreamStatus`** / **`Tape`**: badge renders per mode (`animate-blink` +
   lime only when live); existing empty/row rendering still passes.

## Scope & coordination

- **#148 (SIWE, frontend)** owns `client.ts` structure/headers. This change is
  body-only + one import + one private fn, no `StreamOptions`/interface change.
  If #148 merges first, rebase is trivial (reuse its header builder for the SSE
  request).
- **No env-wiring change.** The tape works identically in mock mode
  (`StoreMockClient.streamAuctions`, timer-driven, never drops) and real mode
  (`RestClient` SSE), selected by the existing
  `NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS` flag.

## Acceptance-criteria mapping

| AC | Where |
|---|---|
| `streamAuctions` returns an async iterable | `RestClient.streamAuctions` (Layer 1) |
| Uses gRPC-web or SSE per what C3 shipped | SSE via fetch + ReadableStream |
| Tape consumes the stream | `useAuctionFeed` → `Tape` |
| Degrades to REST polling (I2.5) on disconnect | `useAuctionHistory` poll re-enabled when `status !== 'live'` |
| Reconnection: exponential backoff + jitter, max 30 s | `_lib/tape/backoff.ts` + `useAuctionStream` |

## Non-goals

- No multi-pair work (single-pair ETH/USDC; gated on backend #29).
- No backend changes (C3's SSE endpoint is consumed as-is).
- No `Last-Event-ID` resumption (the backend sets no event ids); the
  disconnect gap is covered by the degrade-to-polling backfill.
