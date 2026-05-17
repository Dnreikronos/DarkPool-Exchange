# Design Inspirations

This doc names the products we are drawing from and the ones we are
explicitly **not** trying to look like. It is the source of truth for
visual direction — every UI issue (especially F0.5 design tokens, F1.1
layout shell, and the F1.x panels) should be checked against it.

---

## North star

> **The trading UI Hyperliquid would build if privacy were its primary
> feature.**

That gives us three reference points stacked:

1. **Hyperliquid** — density, speed, and number discipline of a serious
   on-chain trading product.
2. **Renegade** — the privacy-first "stealth" aesthetic of a dark-pool
   competitor.
3. **Linear** / **Vercel** — component craft and white-space discipline
   so the density never becomes oppressive.

Not a friendly retail DEX. Not a Bloomberg-terminal museum piece.
Confident, terse, technical, crypto-native.

---

## Direct genre peers (study deeply)

These are the products closest to what we are building. Open them, take
screenshots, copy interaction patterns where appropriate.

### 1. Renegade — https://renegade.fi
**Why:** direct competitor. Privacy-first DEX with MPC matching. Same
narrative ("orders invisible until settlement"). Their landing page
already taught the market the language we will reuse.

What to steal:
- "Stealth" palette: deep, desaturated, single warm accent.
- The way they frame the trust model on the marketing surface.
- Restrained motion. The product feels still until something happens.

What to avoid: their landing is mostly marketing — we are building the
trading app, not the homepage.

### 2. Hyperliquid — https://app.hyperliquid.xyz
**Why:** current gold standard for fast on-chain orderbook trading
UIs. Information-dense without feeling cramped.

What to steal:
- Orderbook with size-weighted depth bars behind the price levels.
- Order ticket density — every field tight, no wasted vertical space.
- Tabular numbers, mono-leaning typography.
- Open-orders and history panels.

What to avoid: Hyperliquid's color palette is loud (greens/reds on
black). Our palette is more restrained.

### 3. CoW Swap — https://swap.cow.fi
**Why:** the only mainstream DEX that already had to explain "batch
auction" to users. They have done the copy work for us.

What to steal:
- Onboarding copy explaining batch auctions, MEV protection, and the
  trust delta vs a normal DEX.
- The way they expose the auction cadence visually.

What to avoid: the cow / playful brand identity. We are not playful.

---

## References per feature

| Feature                  | Primary                          | Secondary                          |
|--------------------------|----------------------------------|------------------------------------|
| Orderbook (#73)          | Hyperliquid                      | OKX, Bybit                         |
| Order entry (#76)        | Hyperliquid order ticket         | GMX, dYdX v4                       |
| Auction tape (#74)       | Renegade (cadence-driven UI)     | Bloomberg time-and-sales           |
| Depth chart (#75)        | Native `lightweight-charts`      | Hyperliquid                        |
| Price chart (#75)        | TradingView                      | —                                  |
| Portfolio / fills (#78)  | Hyperliquid positions panel      | Linear table density               |
| My orders (#77)          | Hyperliquid open orders          | Linear inline-edit tables          |
| Deposit / withdraw (#72) | Phantom modal pattern            | Linear destructive-action dialogs  |
| Onboarding (#79)         | Aztec wallet, Railway Stations   | Linear empty states                |
| Empty states (#79)       | Linear                           | Vercel dashboards                  |
| Toasts (#79)             | Linear                           | Vercel                             |
| Number formatting (F0.5) | TradingView                      | Bloomberg                          |
| Layout shell (#68)       | Hyperliquid 3-column             | Cursor / IDE chrome (for density)  |

---

## Visual language

### Mode
**Dark-first.** Light mode is post-MVP. Don't waste cycles on it now.

### Palette
Near-black background with a cool blue shift; single warm accent.

- **Background:** `oklch(15% 0.01 240)` (almost black, slight blue tint)
- **Surface:** `oklch(18% 0.012 240)` (subtle elevation)
- **Border:** `oklch(25% 0.01 240)` (low-contrast hairlines)
- **Text primary:** `oklch(96% 0.005 240)`
- **Text secondary:** `oklch(70% 0.01 240)`
- **Text tertiary / muted:** `oklch(50% 0.01 240)`
- **Accent — "stealth amber":** `oklch(78% 0.15 75)` (use sparingly — CTAs, active selections, the auction pulse). Evokes the "dark" in dark pool without falling into either generic green-on-black or generic cyan-cyberpunk.
- **Bid green:** `oklch(72% 0.16 145)` desaturated, not WoW green.
- **Ask red:** `oklch(64% 0.18 25)` desaturated, not Casino red.

F0.5 (#66) should commit these as Tailwind tokens. The accent hex is
not load-bearing — pick the closest one your design tool gives. The
intent is *restrained warm against cold near-black*.

### Typography
- **UI / sans:** Inter Variable. `font-feature-settings: "tnum", "ss01"`
  for tabular numbers and the squared-zero stylistic set.
- **Numbers / mono:** JetBrains Mono Variable (or IBM Plex Mono).
  Use for: prices, sizes, addresses, hashes, request IDs.
- **No display font.** Use Inter weight progression for headlines.
- **Small caps for section labels.** `text-xs uppercase tracking-wider
  text-tertiary`. Terminal feel without going full retro.

### Motion principles
1. **Reduce by default.** No tween for tween's sake. The previous
   `front/` was a GSAP showcase — we are not.
2. **Only meaningful motion:**
   - Auction tape pulse on new auction tick.
   - Number flicker on price-level update (very brief opacity nudge).
   - Dialog enter / exit (150ms, ease-out).
   - Skeleton shimmer (slow).
3. **`prefers-reduced-motion` strictly respected** — fall back to
   instant transitions.
4. **No GSAP, no Framer Motion choreography.** Plain CSS transitions
   + the View Transitions API where it earns its keep.

### Tone of copy
- Confident, terse, technical.
- Don't apologise for the semi-custodial model. Explain it once, in
  onboarding, and move on.
- Own the vocabulary: "encrypted order", "batch auction", "clearing
  price", "operator", "settlement".
- Error messages are direct. "Insufficient balance" not "It looks like
  you may not have enough...".

---

## Anti-references

**We are not trying to look like:**

- **PancakeSwap / SushiSwap** — too "DeFi summer", retail, mascot-driven.
- **Coinbase / Robinhood** — fintech-sanitised, friendly. Wrong audience.
- **Phantom / Rainbow** — beautifully designed wallets, but wrong shape
  (consumer wallet, not pro trading tool). Borrow modal patterns, not
  the overall vibe.
- **Bloomberg literal clones** — institutional vibe yes, 1990s aesthetic
  no. Information density comes from typography and rhythm, not from
  cramming 50 widgets on screen.
- **The existing landing inside `front/app/page.tsx`** — it is heavy
  on GSAP entrance animations and shaders, which makes sense for the
  marketing surface but does **not** belong in the trading app at
  `/app`. The landing stays as-is; trading routes follow the motion
  principles in this doc (reduced, meaningful, no GSAP choreography).

---

## Concrete first decisions

These bind work on F0.5 and F1.1. If you disagree, open the issue and
push back — but do it before merging the PR, not after.

### For F0.5 (#66 — design tokens)
- Extend the OKLCH palette above as Tailwind CSS variables in
  `front/app/globals.css` (additive — do not touch the existing landing
  styles).
- Enable `font-variant-numeric: tabular-nums` globally on `body`.
- Load Inter Variable + JetBrains Mono Variable via `next/font/google`
  with `display: swap`.
- shadcn primitives recolored to the palette — no shadcn defaults.

### For F1.1 (#68 — layout shell)
- Sticky header: logo (left), pair selector (center-left), balances
  summary chip (right of center), wallet connect button (far right).
- Three-column grid: `320px | 1fr | 280px` on `>=1280px`; collapses to
  stacked on `<1024px` with order entry behind a Sheet.
- Header height 48px. No tall hero. We dive straight into the data.
- Section labels in mono small caps inside each panel (`ORDERBOOK`,
  `RECENT AUCTIONS`, `MY ORDERS`).
- Hairline dividers (`1px`, border-token), no card shadows. Density
  comes from typography, not chrome.

### For the auction tape (#74)
- Render a vertical strip. Each row: `timestamp · price · size · count`.
- New auctions enter at the top with a single soft pulse on the accent
  color (one-shot, ≤300ms).
- A small "next auction in 3s…" countdown in the panel header
  reinforces the batch cadence — the most distinctive feature of the
  protocol.

### For the orderbook (#73)
- Two columns (bids left, asks right) or two stacked tables — your
  call, but commit to one across the codebase.
- Size bars are background-fills behind the row, not separate elements.
- Hovering a row highlights all rows at the same price tier across
  bids/asks. Clicking autofills the entry form.

### For the order entry (#76)
- Vertical card. Side toggle on top (segmented control). Then price,
  size, total + fee preview, then the place button.
- Multi-stage submission is a single button that mutates its label and
  shows a thin progress bar underneath: `Preparing witness… →
  Generating proof… → Encrypting… → Submitting…`. Do not pop modals
  for each stage.

---

## How to use this doc

When you start work on a UI issue:

1. Open the primary reference URL listed for that feature.
2. Take a screenshot. Drop it in the PR description as the visual
   target.
3. Build to match the *information architecture* and density, not the
   exact pixels — we have our own palette and our own voice.
4. Invoke `frontend-design`, `make-interfaces-feel-better`, and
   `web-design-guidelines` skills as appropriate (see `CLAUDE.md` for
   the curated list).
5. On the PR, link back to this doc if you intentionally deviated, and
   explain why.
