# F1.7 Auction tape (mock-backed) — design

- **Issue:** [#74](https://github.com/Dnreikronos/DarkPool-Exchange/issues/74)
- **Epic:** [#62](https://github.com/Dnreikronos/DarkPool-Exchange/issues/62) Trading App MVP
- **Wave:** 4 (depends on #69 mock store; parallel with #71 balances and #73 orderbook)
- **Branch:** `feat/issue-74-tape`
- **Worktree:** `../darkpool-wt/74-tape`

## 1 · Purpose

Render a vertical strip of recently-settled auctions so the user can
*see* the protocol's batch cadence. The tape is the surface that makes
the dark pool feel alive: rows arrive on the auction tick, prove the
matching engine is producing clearing prices, and give intuition for
the 5-second batch rhythm.

In Phase 1 the tape reads from the in-memory mock store seeded in
#69. Phase 2 swaps the data source to REST (#94 / #95) without
touching any consumer call-site.

## 2 · Scope

### In scope
- `<Tape/>` container component plus subcomponents under
  `front/components/trade/tape/`.
- `useAuctionHistory(limit)` hook reading the mock store via a
  selector.
- Per-row drawer with auction details and a placeholder Etherscan
  slot (filled by #I2.11).
- CSS-only entrance animation, gated by `prefers-reduced-motion`.
- Tabular formatting helpers (relative time, price, volume, count).
- Tests (Vitest + Testing Library) covering formatters, the hook,
  the tape, and the drawer.
- No edits outside `front/components/trade/tape/`. The SDK index
  stays untouched unless implementation reveals a hard need, in
  which case a single additive export line is the only allowed
  exception per the issue's file scope.

### Out of scope
- Wiring the new component into `front/components/trade/Shell.tsx`.
  Shell.tsx belongs to F1.1; this PR ships the tape as an importable
  unit and the integration is a follow-up.
- The auction countdown's underlying tick *source*. The mock store
  already runs `runAuction()` on a 5s interval; the countdown
  derives its display from `recentAuctions[0].timestampUnix` + a
  client-side 1s tick. No store change.
- Streaming (`StreamAuctions`) — that's #95.
- Virtualization. 50 rows × ~28px ≈ 1.4kpx; well below the budget
  where virtualization pays off. Revisit when #94 ships REST.

### Non-goals
- Filtering, sorting, search.
- Per-row action menu (cancel, replicate). My orders panel (#77)
  owns trader-side actions.
- Cross-tab notifications when a fill lands while the user is
  elsewhere. That's a follow-up tied to #77/#78.

## 3 · Acceptance criteria (from issue #74)

- [x] `useAuctionHistory(limit=50)` on mount, polling every 2s.
- [x] Vertical strip showing relative time, clearing price,
      matched volume, match count.
- [x] New entries animate in (slide + fade); respects
      `prefers-reduced-motion`.
- [x] Selecting an auction opens a drawer with a placeholder for
      the Etherscan link.

Polling is satisfied in Phase 1 by subscribing to the Zustand mock
store, which pushes on every state change. The hook's signature
accepts `pollMs` so the Phase 2 REST implementation (#94) can wire
`setInterval` against `client.getAuctionHistory()` without changing
any consumer. The default is `2000`.

## 4 · Architecture

```
                    ┌──────────────────────────────────┐
                    │  mockStore (Zustand singleton)   │
                    │  recentAuctions: AuctionSummary  │
                    │  runAuction() every 5s           │
                    └──────────────┬───────────────────┘
                                   │ subscribe
                                   ▼
              ┌─────────────────────────────────────────┐
              │ useAuctionHistory(limit=50, pollMs=2000)│
              │  selector: recentAuctions.slice(0,limit)│
              │  returns: AuctionSummary[]              │
              └──────────────┬──────────────────────────┘
                             │
                             ▼
        ┌───────────────────────────────────────────┐
        │              <Tape/>                      │
        │  ┌────────────────────────────────────┐   │
        │  │ <Countdown/> · lime · ticker-bar   │   │
        │  ├────────────────────────────────────┤   │
        │  │ <ol> ▼ aria-live="polite"          │   │
        │  │   <TapeRow/> · animate on mount    │   │
        │  │   <TapeRow/>                       │   │
        │  │   ...                              │   │
        │  └────────────────────────────────────┘   │
        └──────────────────┬────────────────────────┘
                           │ onSelect(auction)
                           ▼
              ┌──────────────────────────┐
              │ <TapeDrawer/> · Radix    │
              │  full details +          │
              │  [ ETHERSCAN · PENDING ] │
              └──────────────────────────┘
```

### Data flow

1. `mockStore` (singleton from #69) holds `recentAuctions:
   AuctionSummary[]`, newest-first, capped at 200.
2. `useAuctionHistory(limit)` subscribes via the existing
   `useMockStore` adapter (which delegates to
   `useSyncExternalStore`). The selector returns
   `state.recentAuctions` directly (stable reference unless
   `runAuction()` ran), and slices to `limit` inside `useMemo` keyed
   on the array reference and the head's `auctionId`.
3. `<Tape/>` consumes the hook, renders the header, and maps the
   array to `<TapeRow/>` elements keyed by `auctionId`.
4. Each `<TapeRow/>` is a `<button>` that calls the parent's
   `onSelect`. The parent owns the selected-auction state and the
   drawer's open/closed state.
5. `<TapeDrawer/>` is a `Radix Dialog` mounted as a portal; visible
   when `selectedAuctionId` is set; closing nulls it.

### Render budget

- Selector returns the same reference for 4 of every 5 store
  updates (1s perturb only mutates the orderbook). The slice memo
  catches the remaining no-op renders.
- Rows are pure: `React.memo(TapeRow)` keyed by `auctionId` +
  `nowSecond` (passed as a prop). A new auction re-renders only
  the head row and the freshly-mounted one; the rest get a single
  prop-comparison short-circuit.
- The 1s `useNow()` tick triggers a re-render of the rows because
  relative-time labels update each second. This is the dominant
  cost. Budget: 50 rows × ~3 spans = 150 lightweight text-node
  updates per second. Acceptable; React handles it on the order of
  sub-millisecond.

## 5 · File layout

```
front/components/trade/tape/
├── Tape.tsx                  ← 'use client'; container
├── Countdown.tsx             ← ticker-bar header, lime accent
├── TapeRow.tsx               ← memoized row, opens drawer
├── TapeDrawer.tsx            ← Radix Dialog right-side
├── useAuctionHistory.ts      ← store selector hook
├── useNow.ts                 ← shared 1s tick
├── format.ts                 ← time/price/volume/count formatters
├── tape.module.css           ← @keyframes tape-enter + reduce-motion gate
├── index.ts                  ← barrel: { Tape }
├── format.test.ts
├── useAuctionHistory.test.ts
├── Countdown.test.tsx
└── Tape.test.tsx             ← integration: rows + animation + drawer
```

**File scope guard:** nothing outside `front/components/trade/tape/`
gets modified. `front/lib/sdk/index.ts` likely needs no append; if
during implementation the Phase 2 swap surface demands an export,
that single append is the only allowed file outside the directory.

## 6 · Component contracts

### `useAuctionHistory(opts?)`
```ts
interface UseAuctionHistoryOptions {
  /** Max rows to surface. Default 50. */
  limit?: number
  /**
   * Phase 2 seam. In Phase 1 the store push beats any polling
   * interval; this is documented and accepted as a no-op for the
   * mock backend. Default 2000.
   */
  pollMs?: number
}

function useAuctionHistory(opts?: UseAuctionHistoryOptions): AuctionSummary[]
```
- Reads from the global mock store via `useMockStore`.
- Returns a stable reference across no-op updates.
- Strict-mode safe; no side effects.

### `<Tape/>`
- Props: none. Self-contained: pulls data + state internally.
- Renders `<Countdown/>` followed by `<ol>` of rows.
- Owns the `selectedAuctionId` state and the drawer's open flag.

### `<Countdown/>`
- Props: `latestTimestampMs: number | null`, `intervalMs: number`
  (default 5000).
- Computes `secondsToNext = max(0, ceil((intervalMs - (now -
  latestTimestampMs)) / 1000))`. Shows `[ NEXT AUCTION IN NN ]` in
  `label-lg` tracked uppercase, `tertiary` (lime).
- When `latestTimestampMs` is null (no auctions yet), shows
  `[ WAITING FOR FIRST AUCTION ]` in `secondary`.

### `<TapeRow/>`
- Props: `auction: AuctionSummary`, `nowMs: number`, `onSelect:
  (auctionId: string) => void`, `isNewest: boolean`.
- Renders a 4-col grid: `time price volume count`.
- `time`: `formatRelativeTime` using `nowMs - timestampMs`.
- `price` and `volume`: `formatPrice` / `formatVolume` with
  `tabular-nums`, `body-sm` mono, `primary` color.
- `count`: `formatCount(matchCount)`, `secondary` color.
- Click / Enter / Space: `onSelect(auction.auctionId)`.
- New rows mount with className `styles.enter`; the CSS keyframe
  runs once on mount, then completes.

### `<TapeDrawer/>`
- Props: `auction: AuctionSummary | null`, `onClose: () => void`.
- Rendered always; controlled `open` via Radix `Dialog`. When
  `auction === null`, `open=false`.
- Sliding panel from the right (Radix transitions are CSS-driven;
  we override animations to honor reduced-motion).
- Shows: pair, auction id, full timestamp
  (`HH:MM:SS · DD MMM YYYY`), clearing price, matched volume,
  match count, and `[ ETHERSCAN · PENDING ]` static tag with a
  comment `// filled by I2.11`.

## 7 · Visual / token alignment

Every decision below maps to a token from `DESIGN.md`. No new
colors, no new radii, no shadows beyond the spec's allowed glow
(which we don't use here).

| Surface              | Token / utility                                      |
|----------------------|------------------------------------------------------|
| Tape header          | `ticker-bar` styling: `bg-brand-bg` (`neutral`), 36px, top+bottom `outline` borders |
| Countdown text       | `tertiary` (`#D4FF00`), `label-lg`                  |
| Column headers       | `label-md` uppercase tracked, `on-surface-variant`  |
| Row text (primary)   | `primary` (#FFFFFF), `body-sm`, `tabular-nums`       |
| Row text (meta)      | `on-surface-variant`, `body-sm`, `tabular-nums`      |
| Row hover            | `surface-container` background (per `table-row-hover`)|
| Row separator        | 1px `outline` bottom border                           |
| Drawer surface       | `surface-container`, 32px padding, 1px `outline` left border |
| Drawer backdrop      | `rgba(6,6,10,0.85)`                                   |
| Etherscan placeholder| `tag-bracketed-static`                                |

**Lime budget:** the countdown owns the accent for this surface.
No other element in the tape uses `tertiary`. Newness is signalled
by motion only.

**Timestamp format:** relative — `5s`, `47s`, `1m`, `4m`, `1h`,
`23h`, `2d`. Granularity is intentionally coarse: the design
treats time as a felt rhythm, not a precise reading. Full
`HH:MM:SS · DD MMM YYYY` lives in the drawer.

**Price format:** thousands separator above 10,000 (per
DESIGN-INSPIRATIONS / TradingView convention); 2 decimal places
(USDC quote). Implemented with `decimal.js` toFixed + a
`Intl.NumberFormat`-driven thousands group. No coercion to JS
number.

**Volume format:** 6 decimal places (WETH base), no thousands
separator (volumes < 1 dominate). Right-aligned.

**Count format:** integer, right-aligned, no decimals.

## 8 · Motion contract

### Animation
- New row mounts with class `styles.enter` applied unconditionally.
- CSS:
  ```css
  @media (prefers-reduced-motion: no-preference) {
    .enter {
      animation: tape-enter 240ms cubic-bezier(0.16, 1, 0.3, 1) both;
    }
  }
  @keyframes tape-enter {
    from { opacity: 0; transform: translateY(-6px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  ```
- Older rows do not re-animate because React's reconciliation keys
  by `auctionId`; existing rows just shift down (the layout shift
  is the secondary motion cue).
- Reduced motion: animation rule omitted; rows appear instantly.

### Drawer
- Radix `Dialog.Content` with `data-state` transitions. We override:
  ```css
  @media (prefers-reduced-motion: no-preference) {
    [data-state='open']  { animation: drawer-in  180ms ease-out; }
    [data-state='closed']{ animation: drawer-out 140ms ease-in;  }
  }
  ```
- Reduced motion: no animation; the drawer appears/disappears
  instantly.

## 9 · Accessibility

- `<ol>` with `<li>` per auction. List semantics preserved.
- `aria-live="polite"` on the `<ol>`, `aria-atomic="false"`.
  Screen readers announce newly-inserted auctions without
  re-announcing the whole list.
- Each row is a `<button type="button">` so it is keyboard-
  reachable. `Enter` and `Space` trigger `onSelect` natively.
- Visible focus ring: 1px `tertiary` outline at `outline-offset:
  2px`, matching DESIGN.md's custom focus pattern.
- Drawer: Radix provides focus trap, focus return, ESC-to-close,
  and `aria-labelledby`/`aria-describedby`. We supply a visible
  close button labelled `[ CLOSE ]`.
- Color contrast: row primary text on the canvas measures ≥ 15:1.
  The secondary metadata measures ~3:1 — within DESIGN.md's
  "ambient labels and decorative metadata; not for primary body
  copy" rule.

## 10 · Testing strategy

| Suite                  | What it proves                                           |
|------------------------|----------------------------------------------------------|
| `format.test.ts`       | `formatRelativeTime` ladder (s→m→h→d); price thousands separator above 10k; volume to 6dp; count integer. Decimal-string inputs stay strings (no Number coercion). |
| `useAuctionHistory.test.ts` | Returns latest N; stable reference when store doesn't change `recentAuctions`; updates when `runAuction()` fires; respects custom `limit`. |
| `Countdown.test.tsx`   | Computes seconds-to-next from a fixed `latestTimestampMs` and a mocked `useNow`; shows `[ WAITING ]` when no auctions; never goes negative. |
| `Tape.test.tsx`        | Renders all rows from a seeded mock store; freshly mounted rows receive the `enter` class (existing rows do not re-receive it after a re-render); clicking a row opens the drawer with that auction's details; ESC closes; reduced-motion test asserts the animation class is still applied (CSS does the gating) and the keyframe rule is wrapped in a `(prefers-reduced-motion: no-preference)` media query. |

All tests run under Vitest. The mock store is reset between tests
via `__resetMockStoreSingletonForTests` (already exposed by #69).

## 11 · Verification before PR

Per `superpowers:verification-before-completion`:

1. `npm run test` — all tape suites green.
2. `npm run lint` — clean.
3. `npm run build` — clean.
4. Manual browser pass:
   - Temporarily replace `TapePanel()` body in
     `Shell.tsx` with `<Tape/>` for local verification only.
   - `npm run dev`, navigate to `/app/trade`.
   - Observe a new row entering every ~5s; animation is visible.
   - Toggle DevTools → Rendering → `prefers-reduced-motion:
     reduce`. Reload. Confirm rows appear instantly; no slide-fade.
   - Click a row; drawer opens, ESC closes; focus returns to the
     row.
   - Revert the `Shell.tsx` change before committing. Confirm
     `git status` shows only `front/components/trade/tape/` paths.

Acceptance: every checked item above + AC items from §3.

## 12 · Risks & mitigations

| Risk                                                  | Mitigation |
|-------------------------------------------------------|------------|
| `useNow` 1Hz tick re-renders all rows                 | `React.memo(TapeRow)` with shallow prop comparison. Time string is computed by the parent and passed as a prop, so memoization is meaningful. |
| Selector returns a new array reference on every store update, defeating render skip | Cache the slice with `useMemo` keyed on `recentAuctions` reference + head `auctionId`. Add unit test asserting reference equality across no-op pushes. |
| Animation re-runs on every state change because React reuses the same DOM node | Animation is keyframe-on-mount; React only mounts the node once per `auctionId`. Subsequent layout shifts don't retrigger the keyframe. |
| Radix Dialog's default animation conflicts with reduced-motion | Override with our own CSS gated by the media query. Test asserts `data-state='open'` styles don't contain an animation rule when reduced-motion is on (via `window.matchMedia` mock). |
| Lime in countdown competes with order-entry button when entry is focused | Out of scope for this PR. DESIGN-INSPIRATIONS already documents the migration: when the order-entry form (#76) is in focused state it pulls the accent. We accept it as a follow-up. |

## 13 · Follow-ups (out of this PR)

- Wire `<Tape/>` into `front/components/trade/Shell.tsx`. One-line
  change; file is off-limits to this PR per `docs/PARALLEL-WORK.md`
  scope. The PR body flags this as the only integration step.
- Phase 2 swap (#94): replace `useAuctionHistory`'s store
  subscription with `client.getAuctionHistory()` polling. The
  hook signature already exposes `pollMs` so consumers do not
  change.
- Streaming upgrade (#95): replace polling with the `StreamAuctions`
  RPC. The hook becomes a thin Connect-RPC stream subscriber; same
  return type.
- Etherscan link (`I2.11` / #100): replace the static placeholder
  tag with a real link computed from the on-chain settled-batch
  event.
