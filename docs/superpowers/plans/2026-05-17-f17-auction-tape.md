# F1.7 Auction Tape Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the auction tape panel for `/app/trade` — a vertical strip of recently-settled auctions backed by the F1.2 mock store, with a lime countdown header, per-row drawer, and motion-respectful entrance animation. Closes [#74](https://github.com/Dnreikronos/DarkPool-Exchange/issues/74).

**Architecture:** Pure selector + thin React hook reads `recentAuctions` from the existing Zustand mock store; UI is a `<Countdown/>` ticker-bar header above an `<ol>` of memoized `<TapeRow/>` buttons; a Radix-Dialog `<TapeDrawer/>` opens on row select. Entrance animation is a CSS Module keyframe gated by `prefers-reduced-motion`. No new runtime dependencies.

**Tech Stack:** Next 14 App Router, React 18 (`useSyncExternalStore`, `useTransition`-free), TypeScript, Tailwind v3.4 (existing tokens), CSS Modules (Next built-in), Radix `Dialog` (already in deps), Vitest 4 (existing), Ladle (existing) for visual stories. `Decimal` math via `decimal.js`. Reuses `<NumericText/>` from F0.5 for price/size formatting.

**Spec:** `docs/superpowers/specs/2026-05-17-f17-auction-tape-design.md`

**Worktree:** `/home/mario/darkpool-wt/74-tape` · branch `feat/issue-74-tape`

**File scope (strict):** Only `front/components/trade/tape/*` is created or modified. The SDK index stays untouched. `Shell.tsx`, `mock-store.ts`, and sibling panel directories are off-limits. Manual browser verification (Task 10) edits `Shell.tsx` **locally only** and reverts before commit.

**Testing note (deviation from spec §10):** The project currently has no `@testing-library/react` or `jsdom`; every existing test is a pure module-level Vitest. To stay inside scope (no new deps) and follow established patterns, this plan tests:
- formatters and the selector as pure functions (unit)
- components via Ladle stories (visual)
- the full strip via the temporary Shell.tsx swap (manual)

The deviation is captured in the PR body so reviewers don't expect render-tested React components.

---

## File Structure

All paths under `front/components/trade/tape/`:

| File                       | Responsibility                                                          |
|----------------------------|--------------------------------------------------------------------------|
| `format.ts`                | Pure formatters: `formatRelativeTime`, `formatFullTimestamp`, `formatCount`, `secondsToNextAuction`. |
| `format.test.ts`           | Vitest unit tests for each formatter.                                   |
| `useAuctionHistory.ts`     | `selectLatestAuctions(state, limit)` pure selector + `useAuctionHistory(opts)` React hook. |
| `useAuctionHistory.test.ts`| Vitest unit tests for the selector (deterministic, no React).           |
| `useNow.ts`                | Shared 1Hz tick hook returning `unixSeconds: number`.                   |
| `tape.module.css`          | `@keyframes tape-enter` and `.enter` class, gated by `prefers-reduced-motion`. Also `.drawer-in`/`.drawer-out` overrides. |
| `TapeRow.tsx`              | `React.memo` row: 4-col grid, button semantics, onSelect callback.      |
| `TapeRow.stories.tsx`      | Ladle story: single row, list of rows, hover state, new-row class.      |
| `Countdown.tsx`            | Ticker-bar header, lime text, `[ NEXT AUCTION IN NN ]`.                 |
| `Countdown.stories.tsx`    | Ladle story: full countdown, waiting state, zero-state.                 |
| `TapeDrawer.tsx`           | Radix Dialog wrapping `front/components/ui/dialog`, right-side panel.   |
| `TapeDrawer.stories.tsx`   | Ladle story: open with a seeded auction.                                |
| `Tape.tsx`                 | `'use client'` container: hook + Countdown + ordered list + drawer.    |
| `Tape.stories.tsx`         | Ladle story: full panel with `createMockStore` seeded and `start()`ed.  |
| `index.ts`                 | Barrel: `export { Tape } from './Tape'`.                                |

---

## Task 1: Scaffold directory and barrel

**Files:**
- Create: `front/components/trade/tape/index.ts`
- Create: `front/components/trade/tape/.gitkeep` (removed in Task 2; only for the initial directory commit)

- [ ] **Step 1: Create the directory and an empty barrel**

```bash
cd /home/mario/darkpool-wt/74-tape
mkdir -p front/components/trade/tape
```

Create `front/components/trade/tape/index.ts` with placeholder content (the real export lands in Task 9):

```ts
// Barrel for the auction tape panel (F1.7 / issue #74).
// Populated as components are implemented per
// docs/superpowers/plans/2026-05-17-f17-auction-tape.md.
export {}
```

- [ ] **Step 2: Verify the directory exists and is empty otherwise**

```bash
ls front/components/trade/tape/
```

Expected: `index.ts` only.

- [ ] **Step 3: Commit**

```bash
git add front/components/trade/tape/index.ts
git commit -m "[F1.7] Scaffold auction tape directory"
```

---

## Task 2: format.ts — `formatRelativeTime` (red → green)

**Files:**
- Create: `front/components/trade/tape/format.test.ts`
- Create: `front/components/trade/tape/format.ts`

Timestamps in `AuctionSummary.timestampUnix` are **seconds** (bigint). `useNow()` will return `unixSeconds: number`. All formatter inputs are typed accordingly.

- [ ] **Step 1: Write the failing test for `formatRelativeTime`**

Create `front/components/trade/tape/format.test.ts`:

```ts
import { describe, expect, it } from 'vitest'

import { formatRelativeTime } from './format'

describe('formatRelativeTime', () => {
  // (auctionUnix, nowUnix) — both seconds.
  it('renders a fresh auction as "0s"', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_000)).toBe('0s')
  })

  it('renders sub-minute ages in seconds', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_005)).toBe('5s')
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_059)).toBe('59s')
  })

  it('switches to minutes at and above 60s', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_060)).toBe('1m')
    expect(formatRelativeTime(1_700_000_000n, 1_700_003_599)).toBe('59m')
  })

  it('switches to hours at and above 60m', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_003_600)).toBe('1h')
    expect(formatRelativeTime(1_700_000_000n, 1_700_086_399)).toBe('23h')
  })

  it('switches to days at and above 24h', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_086_400)).toBe('1d')
    expect(formatRelativeTime(1_700_000_000n, 1_700_259_200)).toBe('3d')
  })

  it('clamps negative ages (clock skew) to 0s', () => {
    expect(formatRelativeTime(1_700_000_010n, 1_700_000_000)).toBe('0s')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx vitest run components/trade/tape/format.test.ts
```

Expected: FAIL — `Cannot find module './format'`.

- [ ] **Step 3: Implement `formatRelativeTime`**

Create `front/components/trade/tape/format.ts`:

```ts
// Pure formatters for the auction tape. No React, no DOM.
// Timestamps from AuctionSummary.timestampUnix arrive as seconds (bigint).

const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = 60 * 60
const SECONDS_PER_DAY = 60 * 60 * 24

export function formatRelativeTime(auctionUnixSeconds: bigint, nowUnixSeconds: number): string {
  const ageSeconds = Math.max(0, nowUnixSeconds - Number(auctionUnixSeconds))
  if (ageSeconds < SECONDS_PER_MINUTE) return `${ageSeconds}s`
  if (ageSeconds < SECONDS_PER_HOUR) return `${Math.floor(ageSeconds / SECONDS_PER_MINUTE)}m`
  if (ageSeconds < SECONDS_PER_DAY) return `${Math.floor(ageSeconds / SECONDS_PER_HOUR)}h`
  return `${Math.floor(ageSeconds / SECONDS_PER_DAY)}d`
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
npx vitest run components/trade/tape/format.test.ts
```

Expected: PASS — 6/6 tests green.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/format.ts front/components/trade/tape/format.test.ts
git commit -m "[F1.7] Add formatRelativeTime for tape rows"
```

---

## Task 3: format.ts — `formatFullTimestamp`, `formatCount`, `secondsToNextAuction`

**Files:**
- Modify: `front/components/trade/tape/format.test.ts`
- Modify: `front/components/trade/tape/format.ts`

- [ ] **Step 1: Append failing tests for the remaining formatters**

Append to `front/components/trade/tape/format.test.ts`:

```ts
import { formatCount, formatFullTimestamp, secondsToNextAuction } from './format'

describe('formatFullTimestamp', () => {
  it('renders HH:MM:SS · DD MMM YYYY in UTC', () => {
    // 1700000000 = 2023-11-14 22:13:20 UTC
    expect(formatFullTimestamp(1_700_000_000n)).toBe('22:13:20 · 14 NOV 2023')
  })

  it('zero-pads time components', () => {
    // 1672531200 = 2023-01-01 00:00:00 UTC
    expect(formatFullTimestamp(1_672_531_200n)).toBe('00:00:00 · 01 JAN 2023')
  })
})

describe('formatCount', () => {
  it('renders an integer as-is', () => {
    expect(formatCount(0)).toBe('0')
    expect(formatCount(1)).toBe('1')
    expect(formatCount(42)).toBe('42')
  })

  it('clamps negative inputs to 0', () => {
    expect(formatCount(-3)).toBe('0')
  })
})

describe('secondsToNextAuction', () => {
  it('returns intervalSeconds when no auction has landed yet', () => {
    expect(secondsToNextAuction(null, 1_700_000_000, 5)).toBe(5)
  })

  it('counts down from intervalSeconds after the last auction', () => {
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_000, 5)).toBe(5)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_002, 5)).toBe(3)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_004, 5)).toBe(1)
  })

  it('wraps to a fresh interval after the boundary passes (clock keeps moving past 5s when the mock is paused)', () => {
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_005, 5)).toBe(5)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_006, 5)).toBe(4)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_012, 5)).toBe(3)
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx vitest run components/trade/tape/format.test.ts
```

Expected: FAIL — `formatFullTimestamp is not a function`, etc.

- [ ] **Step 3: Implement the new formatters**

Append to `front/components/trade/tape/format.ts`:

```ts
const MONTHS = [
  'JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN',
  'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC',
] as const

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}

export function formatFullTimestamp(unixSeconds: bigint): string {
  const d = new Date(Number(unixSeconds) * 1000)
  const hh = pad2(d.getUTCHours())
  const mm = pad2(d.getUTCMinutes())
  const ss = pad2(d.getUTCSeconds())
  const day = pad2(d.getUTCDate())
  const mon = MONTHS[d.getUTCMonth()]
  const year = d.getUTCFullYear()
  return `${hh}:${mm}:${ss} · ${day} ${mon} ${year}`
}

export function formatCount(n: number): string {
  return `${Math.max(0, Math.floor(n))}`
}

/**
 * Seconds remaining until the next auction tick. `intervalSeconds` is the
 * cadence configured on the mock store (default 5). When no auction has
 * landed yet (`lastAuctionUnix === null`), returns a full interval so the
 * countdown shows a valid number on first render.
 *
 * If `now` has drifted past `lastAuctionUnix + intervalSeconds` (e.g. the
 * mock store was paused), wraps with `mod` so the countdown stays inside
 * `[1, intervalSeconds]`.
 */
export function secondsToNextAuction(
  lastAuctionUnixSeconds: bigint | null,
  nowUnixSeconds: number,
  intervalSeconds: number
): number {
  if (lastAuctionUnixSeconds === null) return intervalSeconds
  const elapsed = nowUnixSeconds - Number(lastAuctionUnixSeconds)
  if (elapsed < 0) return intervalSeconds
  const remainder = elapsed % intervalSeconds
  return remainder === 0 ? intervalSeconds : intervalSeconds - remainder
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
npx vitest run components/trade/tape/format.test.ts
```

Expected: PASS — all suites green.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/format.ts front/components/trade/tape/format.test.ts
git commit -m "[F1.7] Add timestamp, count, and countdown formatters"
```

---

## Task 4: useAuctionHistory — pure selector

**Files:**
- Create: `front/components/trade/tape/useAuctionHistory.test.ts`
- Create: `front/components/trade/tape/useAuctionHistory.ts`

The hook is split in two: a pure `selectLatestAuctions(state, limit)` that the React hook composes, and the React wrapper itself. Only the selector is unit-tested — the wrapper is a one-liner around `useMockStore`.

- [ ] **Step 1: Write the failing selector test**

Create `front/components/trade/tape/useAuctionHistory.test.ts`:

```ts
import { describe, expect, it } from 'vitest'

import { createMockStore } from '@/lib/mock-store'

import { DEFAULT_AUCTION_HISTORY_LIMIT, selectLatestAuctions } from './useAuctionHistory'

const FROZEN_NOW_SECONDS = 1_700_000_000
const SEED = 11

function freshState(seed = SEED) {
  return createMockStore({
    seed,
    now: () => FROZEN_NOW_SECONDS,
    mid: '3000',
    depth: 4,
    auctionHistory: 8,
  }).getState()
}

describe('selectLatestAuctions', () => {
  it('returns at most `limit` rows, newest first', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state, 5)
    expect(rows).toHaveLength(5)
    // newest-first ordering: each row's timestamp is >= the next
    for (let i = 0; i < rows.length - 1; i++) {
      expect(rows[i].timestampUnix >= rows[i + 1].timestampUnix).toBe(true)
    }
  })

  it('returns the full history when limit exceeds available rows', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state, 999)
    expect(rows).toHaveLength(state.recentAuctions.length)
  })

  it('returns the same reference for identical inputs (memo seam)', () => {
    const state = freshState()
    // The selector itself is referentially stable across calls because it
    // returns a slice of the same source array; the React hook wraps this
    // in a useMemo for the same reason.
    const a = selectLatestAuctions(state, 3)
    const b = selectLatestAuctions(state, 3)
    // Different array instances are OK; per-element identity is what
    // downstream React.memo cares about.
    expect(a[0]).toBe(b[0])
    expect(a[1]).toBe(b[1])
  })

  it('honors the default limit when called without an explicit value', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state)
    expect(rows.length).toBeLessThanOrEqual(DEFAULT_AUCTION_HISTORY_LIMIT)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx vitest run components/trade/tape/useAuctionHistory.test.ts
```

Expected: FAIL — `Cannot find module './useAuctionHistory'`.

- [ ] **Step 3: Implement the selector and the hook**

Create `front/components/trade/tape/useAuctionHistory.ts`:

```ts
'use client'

import { useMemo } from 'react'

import type { AuctionSummary } from '@/lib/sdk'
import { type MockStoreState, useMockStore } from '@/lib/mock-store'

export const DEFAULT_AUCTION_HISTORY_LIMIT = 50

/**
 * Pure selector over the mock store. Returns the newest `limit` auctions,
 * preserving the store's newest-first ordering. Exported separately so it
 * can be unit-tested without a React renderer.
 */
export function selectLatestAuctions(
  state: MockStoreState,
  limit: number = DEFAULT_AUCTION_HISTORY_LIMIT
): readonly AuctionSummary[] {
  if (state.recentAuctions.length <= limit) return state.recentAuctions
  return state.recentAuctions.slice(0, limit)
}

export interface UseAuctionHistoryOptions {
  /** Max rows to surface. Defaults to {@link DEFAULT_AUCTION_HISTORY_LIMIT}. */
  limit?: number
  /**
   * Phase 2 seam. In Phase 1 the store push beats any polling interval;
   * this is a documented no-op for the mock backend. Default 2000.
   * The REST swap (#94) will reuse this prop with setInterval.
   */
  pollMs?: number
}

/**
 * Subscribes to the mock store and returns the latest `limit` auctions.
 * The returned array is referentially stable when the underlying
 * `recentAuctions` reference is unchanged — important for `React.memo`
 * row components, which would otherwise re-render on every 1s perturb.
 */
export function useAuctionHistory(opts: UseAuctionHistoryOptions = {}): readonly AuctionSummary[] {
  const limit = opts.limit ?? DEFAULT_AUCTION_HISTORY_LIMIT
  const recentAuctions = useMockStore((s) => s.recentAuctions)
  return useMemo(() => {
    if (recentAuctions.length <= limit) return recentAuctions
    return recentAuctions.slice(0, limit)
  }, [recentAuctions, limit])
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
npx vitest run components/trade/tape/useAuctionHistory.test.ts
```

Expected: PASS — 4/4 tests green.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/useAuctionHistory.ts front/components/trade/tape/useAuctionHistory.test.ts
git commit -m "[F1.7] Add useAuctionHistory selector + hook"
```

---

## Task 5: useNow — shared 1Hz tick

**Files:**
- Create: `front/components/trade/tape/useNow.ts`

Trivially small. No unit test; covered indirectly by Tape rendering correctly under Ladle and the browser swap.

- [ ] **Step 1: Implement the hook**

Create `front/components/trade/tape/useNow.ts`:

```ts
'use client'

import { useEffect, useState } from 'react'

/**
 * Returns the current Unix time in seconds and updates once per second.
 * Single shared subscription pattern: every consumer mounts its own
 * interval, but the cost is one setState per second per consumer —
 * negligible for the tape (one parent component).
 *
 * Pass `nowSecondsOverride` (tests, Ladle stories) to freeze time.
 */
export function useNow(nowSecondsOverride?: number): number {
  const [now, setNow] = useState<number>(() =>
    nowSecondsOverride ?? Math.floor(Date.now() / 1000)
  )
  useEffect(() => {
    if (nowSecondsOverride !== undefined) return
    const id = setInterval(() => {
      setNow(Math.floor(Date.now() / 1000))
    }, 1000)
    return () => clearInterval(id)
  }, [nowSecondsOverride])
  return now
}
```

- [ ] **Step 2: Type-check**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx tsc --noEmit
```

Expected: no errors related to `useNow.ts`.

- [ ] **Step 3: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/useNow.ts
git commit -m "[F1.7] Add useNow 1Hz tick hook"
```

---

## Task 6: tape.module.css — keyframes gated by reduced-motion

**Files:**
- Create: `front/components/trade/tape/tape.module.css`

- [ ] **Step 1: Author the stylesheet**

Create `front/components/trade/tape/tape.module.css`:

```css
/*
 * Keyframes for the tape row entrance and drawer transitions.
 * Both are gated by `prefers-reduced-motion: no-preference` so that
 * users who request reduced motion get instant appearance.
 */

@keyframes tape-enter {
  from {
    opacity: 0;
    transform: translateY(-6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes drawer-in {
  from {
    opacity: 0;
    transform: translateX(8px);
  }
  to {
    opacity: 1;
    transform: translateX(0);
  }
}

@keyframes drawer-out {
  from {
    opacity: 1;
    transform: translateX(0);
  }
  to {
    opacity: 0;
    transform: translateX(8px);
  }
}

.enter {
  /* No animation when reduced-motion is requested — rows appear instantly. */
}

.drawer {
  /* Default no animation; the media query below enables it. */
}

@media (prefers-reduced-motion: no-preference) {
  .enter {
    animation: tape-enter 240ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .drawer[data-state='open'] {
    animation: drawer-in 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .drawer[data-state='closed'] {
    animation: drawer-out 140ms ease-in;
  }
}
```

- [ ] **Step 2: Verify the file parses (Next dev build does this on first import; pre-commit type-check skips CSS)**

No standalone command; correctness is exercised by Tasks 7–10 importing it.

- [ ] **Step 3: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/tape.module.css
git commit -m "[F1.7] Add tape keyframes gated by prefers-reduced-motion"
```

---

## Task 7: TapeRow component + Ladle story

**Files:**
- Create: `front/components/trade/tape/TapeRow.tsx`
- Create: `front/components/trade/tape/TapeRow.stories.tsx`

- [ ] **Step 1: Implement `TapeRow`**

Create `front/components/trade/tape/TapeRow.tsx`:

```tsx
'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import type { AuctionSummary } from '@/lib/sdk'

import { formatCount, formatRelativeTime } from './format'
import styles from './tape.module.css'

export interface TapeRowProps {
  auction: AuctionSummary
  /** Current Unix seconds; passed down so all rows share one tick source. */
  nowUnixSeconds: number
  /** Click / Enter / Space handler. */
  onSelect: (auctionId: string) => void
}

function TapeRowImpl({ auction, nowUnixSeconds, onSelect }: TapeRowProps): JSX.Element {
  const handleClick = React.useCallback(() => {
    onSelect(auction.auctionId)
  }, [auction.auctionId, onSelect])

  return (
    <li className="border-b border-brand-border">
      <button
        type="button"
        onClick={handleClick}
        className={`${styles.enter} grid w-full grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)_2rem] items-center gap-3 px-4 py-2 text-left font-mono text-body-sm tabular-nums text-brand-fg transition-colors hover:bg-brand-surface focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent`}
        aria-label={`Auction ${auction.auctionId}, clearing price ${auction.clearingPrice}`}
      >
        <span className="text-brand-muted">
          {formatRelativeTime(auction.timestampUnix, nowUnixSeconds)}
        </span>
        <NumericText
          value={auction.clearingPrice}
          kind="price"
          align="right"
          className="text-brand-fg"
        />
        <NumericText
          value={auction.matchedVolume}
          kind="size"
          align="right"
          className="text-brand-fg"
        />
        <span className="text-right text-brand-muted">{formatCount(auction.matchCount)}</span>
      </button>
    </li>
  )
}

export const TapeRow = React.memo(TapeRowImpl)
TapeRow.displayName = 'TapeRow'
```

- [ ] **Step 2: Write a Ladle story exercising the row**

Create `front/components/trade/tape/TapeRow.stories.tsx`:

```tsx
import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk'

import { TapeRow } from './TapeRow'

const NOW = 1_700_000_120

function mkAuction(opts: Partial<{
  auctionId: string
  clearingPrice: string
  matchedVolume: string
  matchCount: number
  ageSeconds: number
}> = {}) {
  return create(AuctionSummarySchema, {
    auctionId: opts.auctionId ?? 'a-001',
    pair: 'ETH/USDC',
    clearingPrice: opts.clearingPrice ?? '2418.10',
    matchedVolume: opts.matchedVolume ?? '0.0453',
    matchCount: opts.matchCount ?? 3,
    timestampUnix: BigInt(NOW - (opts.ageSeconds ?? 5)),
  })
}

const noop = (_id: string): void => undefined

export const SingleRow = () => (
  <ol className="w-[320px] border border-brand-border bg-brand-bg">
    <TapeRow auction={mkAuction()} nowUnixSeconds={NOW} onSelect={noop} />
  </ol>
)

export const ListOfRows = () => (
  <ol className="w-[320px] border border-brand-border bg-brand-bg">
    <TapeRow
      auction={mkAuction({ auctionId: 'a-100', ageSeconds: 2, clearingPrice: '2419.85', matchedVolume: '0.0121', matchCount: 1 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-099', ageSeconds: 7, clearingPrice: '12345.6789', matchedVolume: '0.089', matchCount: 5 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-098', ageSeconds: 17, matchedVolume: '0', matchCount: 0 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-097', ageSeconds: 65, clearingPrice: '2417.42', matchedVolume: '0.0023', matchCount: 1 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
  </ol>
)
```

- [ ] **Step 3: Run Ladle to verify rows render correctly**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx ladle serve --port 61000 &
LADLE_PID=$!
sleep 4
curl -sf http://localhost:61000/ > /dev/null && echo "Ladle up"
kill $LADLE_PID 2>/dev/null
```

Expected: `Ladle up` printed. (Visual inspection of `TapeRow` happens in Task 10.)

- [ ] **Step 4: Type-check**

```bash
npx tsc --noEmit
```

Expected: no errors in `TapeRow.tsx` or `TapeRow.stories.tsx`.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/TapeRow.tsx front/components/trade/tape/TapeRow.stories.tsx
git commit -m "[F1.7] Add TapeRow with NumericText columns"
```

---

## Task 8: Countdown component + Ladle story

**Files:**
- Create: `front/components/trade/tape/Countdown.tsx`
- Create: `front/components/trade/tape/Countdown.stories.tsx`

- [ ] **Step 1: Implement `Countdown`**

Create `front/components/trade/tape/Countdown.tsx`:

```tsx
'use client'

import * as React from 'react'

import { secondsToNextAuction } from './format'

export interface CountdownProps {
  latestAuctionUnixSeconds: bigint | null
  nowUnixSeconds: number
  /** Cadence of the auction tick. Mock store defaults to 5s. */
  intervalSeconds?: number
}

const DEFAULT_INTERVAL_SECONDS = 5

export function Countdown({
  latestAuctionUnixSeconds,
  nowUnixSeconds,
  intervalSeconds = DEFAULT_INTERVAL_SECONDS,
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
      className={`flex h-9 items-center justify-center border-b border-brand-border bg-brand-bg px-4 font-mono text-label-lg uppercase tracking-label ${
        waiting ? 'text-brand-muted' : 'text-brand-accent'
      }`}
    >
      {label}
    </div>
  )
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}
```

- [ ] **Step 2: Write a Ladle story**

Create `front/components/trade/tape/Countdown.stories.tsx`:

```tsx
import * as React from 'react'

import { Countdown } from './Countdown'

const NOW = 1_700_000_000

export const Waiting = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={null} nowUnixSeconds={NOW} />
  </div>
)

export const JustAfterAuction = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW)} nowUnixSeconds={NOW} />
  </div>
)

export const HalfwayToNext = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW - 2)} nowUnixSeconds={NOW} />
  </div>
)

export const OneSecondLeft = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW - 4)} nowUnixSeconds={NOW} />
  </div>
)
```

- [ ] **Step 3: Type-check**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/Countdown.tsx front/components/trade/tape/Countdown.stories.tsx
git commit -m "[F1.7] Add Countdown ticker-bar header"
```

---

## Task 9: TapeDrawer component + Ladle story

**Files:**
- Create: `front/components/trade/tape/TapeDrawer.tsx`
- Create: `front/components/trade/tape/TapeDrawer.stories.tsx`

The drawer overrides the centered positioning of `front/components/ui/dialog`'s `DialogContent` by adding right-anchored Tailwind classes. The Radix portal + overlay are reused as-is.

- [ ] **Step 1: Implement `TapeDrawer`**

Create `front/components/trade/tape/TapeDrawer.tsx`:

```tsx
'use client'

import * as React from 'react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import type { AuctionSummary } from '@/lib/sdk'

import { formatFullTimestamp } from './format'
import styles from './tape.module.css'

export interface TapeDrawerProps {
  auction: AuctionSummary | null
  onClose: () => void
}

export function TapeDrawer({ auction, onClose }: TapeDrawerProps): JSX.Element {
  const open = auction !== null
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      <DialogContent
        className={`${styles.drawer} fixed left-auto right-0 top-0 h-screen w-full max-w-[420px] translate-x-0 translate-y-0 border-l border-brand-border bg-brand-surface p-8`}
        aria-describedby={undefined}
      >
        {auction && (
          <>
            <DialogTitle className="font-display text-display-sm uppercase">
              [ AUCTION {auction.auctionId} ]
            </DialogTitle>
            <DialogDescription className="mt-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
              {auction.pair}
            </DialogDescription>

            <dl className="mt-8 grid grid-cols-[max-content_minmax(0,1fr)] gap-y-3 font-mono text-body-sm">
              <Row label="TIMESTAMP" value={formatFullTimestamp(auction.timestampUnix)} mono />
              <Row label="CLEARING" value={auction.clearingPrice} mono />
              <Row label="VOLUME" value={auction.matchedVolume} mono />
              <Row label="MATCHES" value={String(auction.matchCount)} mono />
            </dl>

            <div className="mt-10 flex flex-col gap-2">
              <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
                ETHERSCAN
              </span>
              <span
                aria-label="Etherscan link pending Phase 2 integration"
                className="font-mono text-label-lg uppercase tracking-label text-brand-muted"
              >
                [ ETHERSCAN · PENDING ]
              </span>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }): JSX.Element {
  return (
    <>
      <dt className="pr-4 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {label}
      </dt>
      <dd className={`${mono ? 'tabular-nums' : ''} text-brand-fg`}>{value}</dd>
    </>
  )
}
```

- [ ] **Step 2: Ladle story for the open drawer**

Create `front/components/trade/tape/TapeDrawer.stories.tsx`:

```tsx
import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk'

import { TapeDrawer } from './TapeDrawer'

const sample = create(AuctionSummarySchema, {
  auctionId: '0xa1b2c3-1042',
  pair: 'ETH/USDC',
  clearingPrice: '2418.10',
  matchedVolume: '0.045300',
  matchCount: 3,
  timestampUnix: 1_700_000_000n,
})

export const Open = () => {
  const [auction, setAuction] = React.useState<typeof sample | null>(sample)
  return (
    <div className="min-h-screen bg-brand-bg p-8">
      <button
        type="button"
        onClick={() => setAuction(sample)}
        className="bg-brand-accent px-4 py-2 font-mono text-label-lg uppercase text-brand-on-accent"
      >
        [ OPEN DRAWER ]
      </button>
      <TapeDrawer auction={auction} onClose={() => setAuction(null)} />
    </div>
  )
}
```

- [ ] **Step 3: Type-check**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/TapeDrawer.tsx front/components/trade/tape/TapeDrawer.stories.tsx
git commit -m "[F1.7] Add TapeDrawer right-anchored Radix panel"
```

---

## Task 10: Tape container + Ladle story + barrel export

**Files:**
- Create: `front/components/trade/tape/Tape.tsx`
- Create: `front/components/trade/tape/Tape.stories.tsx`
- Modify: `front/components/trade/tape/index.ts`

- [ ] **Step 1: Implement `Tape`**

Create `front/components/trade/tape/Tape.tsx`:

```tsx
'use client'

import * as React from 'react'

import type { AuctionSummary } from '@/lib/sdk'

import { Countdown } from './Countdown'
import { TapeDrawer } from './TapeDrawer'
import { TapeRow } from './TapeRow'
import { useAuctionHistory } from './useAuctionHistory'
import { useNow } from './useNow'

export interface TapeProps {
  limit?: number
}

export function Tape({ limit }: TapeProps = {}): JSX.Element {
  const auctions = useAuctionHistory({ limit })
  const nowSeconds = useNow()
  const [selectedId, setSelectedId] = React.useState<string | null>(null)

  const selected = React.useMemo<AuctionSummary | null>(() => {
    if (selectedId === null) return null
    return auctions.find((a) => a.auctionId === selectedId) ?? null
  }, [auctions, selectedId])

  const latestUnix: bigint | null = auctions.length > 0 ? auctions[0].timestampUnix : null

  return (
    <div className="flex h-full min-h-[200px] flex-col">
      <Countdown latestAuctionUnixSeconds={latestUnix} nowUnixSeconds={nowSeconds} />
      <TableHeader />
      {auctions.length === 0 ? (
        <EmptyState />
      ) : (
        <ol aria-live="polite" aria-atomic="false" className="flex-1 overflow-y-auto">
          {auctions.map((a) => (
            <TapeRow
              key={a.auctionId}
              auction={a}
              nowUnixSeconds={nowSeconds}
              onSelect={setSelectedId}
            />
          ))}
        </ol>
      )}
      <TapeDrawer auction={selected} onClose={() => setSelectedId(null)} />
    </div>
  )
}

function TableHeader(): JSX.Element {
  return (
    <div className="grid grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)_2rem] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
      <span>TIME</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">VOLUME</span>
      <span className="text-right">MATCH</span>
    </div>
  )
}

function EmptyState(): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
      [ NO AUCTIONS YET ]
    </div>
  )
}
```

- [ ] **Step 2: Update the barrel**

Overwrite `front/components/trade/tape/index.ts`:

```ts
// Public surface for the auction tape panel (F1.7 / issue #74).
// Consumers import { Tape } and mount it where the panel belongs in
// the trading shell.
export { Tape } from './Tape'
export type { TapeProps } from './Tape'
```

- [ ] **Step 3: Ladle story driving the full panel with a live mock store**

Create `front/components/trade/tape/Tape.stories.tsx`:

```tsx
import * as React from 'react'

import { mockStore } from '@/lib/mock-store'

import { Tape } from './Tape'

/**
 * Renders the live tape against the singleton mock store, starting and
 * stopping its tick loop on mount/unmount. Reload the story to reseed.
 */
export const Live = () => {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 5000 })
    return () => mockStore.getState().stop()
  }, [])

  return (
    <div className="flex h-[600px] w-[360px] flex-col border border-brand-border bg-brand-bg">
      <Tape />
    </div>
  )
}

export const SmallLimit = () => {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 3000 })
    return () => mockStore.getState().stop()
  }, [])

  return (
    <div className="flex h-[400px] w-[360px] flex-col border border-brand-border bg-brand-bg">
      <Tape limit={5} />
    </div>
  )
}
```

- [ ] **Step 4: Type-check**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Run Ladle and confirm the stories load**

```bash
npx ladle serve --port 61000 &
LADLE_PID=$!
sleep 5
curl -sf "http://localhost:61000/" > /dev/null && echo "Ladle up — open http://localhost:61000/ and visit Tape > Live"
# Manual: load the URL, watch a new row arrive every ~5s, click a row, confirm the drawer slides in from the right.
kill $LADLE_PID 2>/dev/null
```

Visual checklist (perform manually; record observations in the PR body):
- [ ] Header reads `[ NEXT AUCTION IN NN ]` in lime, counting down each second.
- [ ] New rows slide+fade in at the top every ~5s.
- [ ] Older rows shift down without re-animating.
- [ ] Clicking any row opens the right-side drawer with full details and `[ ETHERSCAN · PENDING ]`.
- [ ] `Escape` closes the drawer; focus returns to the activated row.
- [ ] DevTools → Rendering → toggle `prefers-reduced-motion: reduce` → reload → rows appear instantly, drawer transition is instant.

- [ ] **Step 6: Commit**

```bash
cd /home/mario/darkpool-wt/74-tape
git add front/components/trade/tape/Tape.tsx front/components/trade/tape/Tape.stories.tsx front/components/trade/tape/index.ts
git commit -m "[F1.7] Wire Tape container + barrel export"
```

---

## Task 11: Browser verification via temporary Shell swap

**Files (LOCAL ONLY — revert before next commit):**
- Modify: `front/components/trade/Shell.tsx`

This step proves the component composes inside the real shell. The edit is reverted before any further commit so the PR keeps zero diff in `Shell.tsx`.

- [ ] **Step 1: Apply the local-only swap**

Open `front/components/trade/Shell.tsx`. Find:

```tsx
function TapePanel() {
  return <Panel label="AUCTION TAPE" icon={TapeGlyph} empty="NO AUCTIONS YET · F1.7" />
}
```

Replace with (LOCAL ONLY — also add the two `import` lines at the top of `Shell.tsx`):

```tsx
import * as React from 'react'
import { mockStore } from '@/lib/mock-store'
import { Tape } from './tape'

function TapePanel() {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 5000 })
    return () => mockStore.getState().stop()
  }, [])
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 items-center gap-3 border-b border-brand-border px-4">
        <TapeGlyph className="text-brand-muted" />
        <span className="font-mono text-label-md uppercase text-brand-muted">AUCTION TAPE</span>
      </div>
      <div className="flex-1">
        <Tape />
      </div>
    </div>
  )
}
```

**This whole edit is a local-only diff — do not commit.** `React` is already imported as a side-effect via the `'use client'` directive's React 17+ JSX runtime, but explicitly importing it for `useEffect` is the safest path.

- [ ] **Step 2: Run the dev server and verify in the browser**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npm run dev
```

Then in Chrome / Firefox:
1. Navigate to `http://localhost:3000/app/trade`.
2. Confirm the right column shows the tape, with countdown header, table header, and rows arriving every ~5s.
3. Click a row → drawer slides in from the right with the auction's details and `[ ETHERSCAN · PENDING ]`.
4. ESC closes the drawer; focus returns to the row.
5. Open DevTools → ⋮ → More tools → Rendering → set `prefers-reduced-motion` to `reduce`. Reload the page. Confirm:
   - New rows appear without slide/fade animation.
   - The drawer opens/closes instantly with no transform animation.
   - The blinking pill / countdown text still updates (these are content updates, not motion).

Document each ✅ in the PR body.

- [ ] **Step 3: Stop the dev server**

`Ctrl+C` in the terminal.

- [ ] **Step 4: Revert the local Shell.tsx swap**

```bash
cd /home/mario/darkpool-wt/74-tape
git checkout -- front/components/trade/Shell.tsx
git status
```

Expected: `git status` shows no modifications to `Shell.tsx`. The only changes in the tree should be unstaged test/dev artifacts (if any). If you mounted the tick loop in a different file, revert that too. **No commit if `git status` is clean.**

- [ ] **Step 5: Confirm the diff against `origin/main` only contains tape files and the spec**

```bash
git diff --stat origin/main...HEAD
```

Expected output lists only:
- `docs/superpowers/specs/2026-05-17-f17-auction-tape-design.md`
- `docs/superpowers/plans/2026-05-17-f17-auction-tape.md`
- `front/components/trade/tape/*`

No other path. If any other path appears, investigate and revert.

---

## Task 12: Full verification suite

**Files:** none modified.

- [ ] **Step 1: Run the project test suite**

```bash
cd /home/mario/darkpool-wt/74-tape/front
npm run test
```

Expected: all suites green, including the new `format.test.ts` and `useAuctionHistory.test.ts`.

- [ ] **Step 2: Lint**

```bash
npm run lint
```

Expected: no errors. Warnings only if pre-existing.

- [ ] **Step 3: Type-check via build (catches CSS module imports, JSX paths)**

```bash
npm run build
```

Expected: build completes; no TypeScript errors; no missing-module errors.

- [ ] **Step 4: Rebase on `origin/main` and re-run the suite**

```bash
cd /home/mario/darkpool-wt/74-tape
git fetch origin
git rebase origin/main
cd front
npm install   # in case package.json was changed upstream
npm run test
npm run lint
npm run build
```

Expected: all green. If conflicts surface outside `front/components/trade/tape/*`, stop and surface on the issue per `docs/PARALLEL-WORK.md`.

---

## Task 13: Push branch, open PR

- [ ] **Step 1: Push the branch**

```bash
cd /home/mario/darkpool-wt/74-tape
git push -u origin feat/issue-74-tape
```

- [ ] **Step 2: Open the PR**

```bash
gh pr create --title "[F1.7] Auction tape (mock-backed)" --body "$(cat <<'EOF'
Closes #74

## Summary
- New tape panel under \`front/components/trade/tape/\` reading the F1.2 mock store via \`useAuctionHistory\`.
- \`<Countdown/>\` ticker-bar header owns the lime accent for this surface; row newness is signalled by a CSS-keyframe slide+fade gated by \`prefers-reduced-motion\`.
- Per-row drawer (\`<TapeDrawer/>\`) shows full auction details plus a \`[ ETHERSCAN · PENDING ]\` placeholder for #I2.11.

## Design references
- Spec: \`docs/superpowers/specs/2026-05-17-f17-auction-tape-design.md\`
- Plan: \`docs/superpowers/plans/2026-05-17-f17-auction-tape.md\`

## Testing strategy (deviation from the spec)
The project has no \`@testing-library/react\` / \`jsdom\`. Adding those would be out of scope. So:
- \`format.test.ts\` — relative-time, full-timestamp, count, countdown.
- \`useAuctionHistory.test.ts\` — pure selector.
- React components — Ladle stories (\`*.stories.tsx\`) + manual browser verification through a temporary \`Shell.tsx\` swap (reverted before commit).

## Out of scope / follow-ups
- Wiring \`<Tape/>\` into \`Shell.tsx\`. That file belongs to F1.1; one-line integration is a follow-up.
- Phase 2 REST swap (#94) — \`useAuctionHistory.pollMs\` is the documented seam.
- Streaming upgrade (#95).
- Real Etherscan link (#I2.11 / #100).

## Verified manually
- [x] Rows arrive every ~5s; head row animates in.
- [x] Click row → drawer opens with details and ETHERSCAN placeholder.
- [x] ESC closes drawer; focus returns to row.
- [x] \`prefers-reduced-motion: reduce\` → animations skipped; instant appearance.
EOF
)"
```

- [ ] **Step 3: Return the PR URL**

`gh pr view --json url --jq .url` and surface the URL.

---

## Spec coverage self-check

| Spec requirement                                                | Task |
|-----------------------------------------------------------------|------|
| `useAuctionHistory(limit=50)` reading the mock store            | 4    |
| Polling-every-2s seam documented for Phase 2                    | 4    |
| Vertical strip with relative time / clearing price / matched volume / match count | 7, 10 |
| New entries animate in (slide + fade)                           | 6, 7 |
| Respects `prefers-reduced-motion`                               | 6, 11 |
| Row select opens drawer with Etherscan placeholder              | 9, 10 |
| Lime accent reserved for countdown header                       | 8    |
| Tabular numbers, decimals stay strings                          | 7 (via `NumericText`) |
| Tests for formatters and selector                               | 2, 3, 4 |
| Ladle stories for visual review                                 | 7, 8, 9, 10 |
| Verification gate (lint / build / test / browser)               | 11, 12 |
| One PR, `Closes #74` body, no Claude co-author                  | 13   |

Every spec section maps to a task. No placeholders, no TODOs.
