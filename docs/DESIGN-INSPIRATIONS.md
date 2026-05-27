# Design Inspirations

## How to read this doc

The **source of truth** for the visual system — palette, typography,
spacing, components, motion contract, do's and don'ts — is
[`DESIGN.md`](../DESIGN.md) at the repo root. Tokens come from there;
don't redefine them here. If something below contradicts `DESIGN.md`,
**`DESIGN.md` wins**.

This doc covers what `DESIGN.md` deliberately does not:

- **Trading-specific tensions** the design system raises (bid/ask
  without semantic colors, accent budget on dense screens, no-icon
  rule applied to wallets and tokens).
- **Per-feature UX references** — *which* products to study for *which*
  interaction patterns, separated from their brand identity.
- **Tone of copy** for transactional surfaces (errors, status,
  multi-stage feedback).
- **Patterns to skip** — concrete habits to avoid, not products to
  blacklist.
- **First-pass decisions for trading panels**, all expressed in
  `DESIGN.md` tokens.

If you are picking up F0.5 (#66, design tokens) or any F1.x panel,
read `DESIGN.md` first, then this.

---

## Trading tensions the design system raises

### Bid / ask without green and red

`DESIGN.md` forbids semantic colors and a second accent. The orderbook
can't fall back to the usual green-bid / red-ask. Differentiation comes
from **position + typography + opacity**, never hue:

- **Position:** bids on the left column, asks on the right (or stacked
  bids-on-top / asks-on-bottom on narrow viewports). The split is the
  primary cue.
- **Bracketed prefix on the column header:** `[ BIDS ]` and `[ ASKS ]`
  in `tag-bracketed-static`. Reinforces the position cue, fits the
  bracketed-tag pattern already in the system.
- **Cumulative-size bars:** background fills behind rows, drawn in
  `primary` at 8–12% opacity for bids and `secondary` at 18–22% opacity
  for asks. Different luminance, no new hue.
- **Best bid / best ask row:** rendered in `body-md` (one step up from
  `body-sm` used for the rest of the rows). Hierarchy through size, not
  color.
- **Last-trade price tick** (the marker between bid and ask): a single
  row in `display-sm` Bebas Neue, white. This is the one place a chart
  reader looks first and it earns the larger type.

### Accent budget per view

DESIGN.md: "use `tertiary` for exactly one element per view." Trading
screens are dense. Pick the accent **per surface** ahead of time and
stick to it:

| Surface          | Lime goes to                                          |
|------------------|--------------------------------------------------------|
| `/app/trade`     | The countdown to the next auction in the header.      |
| `/app/trade` (order-entry focused state) | Temporarily the **Place** button while the form is valid and not submitting. |
| `/app/portfolio` | The unrealized P&L value (if positive) — single stat. |
| Deposit modal    | The **Deposit** primary button.                       |
| Onboarding step  | The current step number in `step-node`.               |

The auction countdown is the default because it is the most distinctive
behavior of the protocol and the thing a user should learn to watch.
When the user focuses the order-entry form, the accent migrates to the
Place button. Never two lime elements on screen.

### No icons

DESIGN.md: identity comes from typography + box-drawing chars
(`█ ░ ─`), not icons. Concrete consequences for trading:

- **Wallet picker:** `[ METAMASK ]`, `[ RAINBOW ]`, `[ WALLETCONNECT ]`
  as bracketed-tag buttons. No wallet logos.
- **Token selector:** `[ USDC ]`, `[ WETH ]` as bracketed tags. No
  token icons.
- **Chain badge:** `[ ARBITRUM · 42161 ]` in `tag-bracketed-static`.
- **Status indicators:** `status-pill-live` (square 6×6, blinking) +
  bracketed label, never a green-checkmark or yellow-warning icon.
- **Loading:** box-drawing skeletons (`███░░░`, `░██░██░`) animated
  through the row, not a spinner.
- **Empty states:** ASCII illustration (a single box-drawing block, or
  literally `[ NO ORDERS ]` in `label-lg`), not a friendly mascot.

---

## Per-feature UX references

These are products to study for *interaction patterns and information
architecture*. Visual identity is governed by `DESIGN.md` — borrow
mechanics, not aesthetics.

| Feature                  | Primary reference                | What to steal                              |
|--------------------------|----------------------------------|--------------------------------------------|
| Orderbook (#73)          | Hyperliquid                      | Cumulative-size bars as row backgrounds; price-tier hover sync across bids/asks; click-to-fill |
| Orderbook (secondary)    | Bybit                            | Aggregation toggle (1, 5, 10 tick grouping) for sparse books |
| Order entry (#76)        | Hyperliquid order ticket         | Density (every field tight); side toggle as segmented control; total + fee preview in the same row |
| Order entry (secondary)  | dYdX v4                          | Multi-stage button feedback (mutates label + thin progress bar instead of stacked modals) |
| Auction tape (#74)       | Renegade                         | Cadence-driven UI — tape ticks visibly on each new auction, signalling the protocol is alive |
| Auction tape (secondary) | Bloomberg time-and-sales         | Column rhythm and timestamp formatting; "tape" as a vertical strip, not a horizontal scroll |
| Depth chart (#75)        | `lightweight-charts` native theming | Mirror the orderbook colors (`primary` + `secondary` opacity steps), no hue addition |
| Price chart (#75)        | TradingView (`lightweight-charts`) | Time-frame presets (1m/5m/1h) as `button-ghost` row above the chart |
| Portfolio / fills (#78)  | Hyperliquid positions panel      | Position + avg entry + unrealized P&L grouped per token in a `stat-value` triplet |
| Portfolio (secondary)    | Linear list density              | Compact rows with `body-sm` and 12×16 padding; no zebra striping (matches `table-row`) |
| My orders (#77)          | Hyperliquid open orders          | Inline cancel button, status pill, single-row layout |
| Deposit / withdraw (#72) | Phantom modal flow               | Multi-step transaction flow with stage labels; error mapping per revert reason; never a generic "Transaction failed" |
| Token selector (in #76)  | PancakeSwap                      | Search-as-you-type with mono ticker as the primary token name (palette ours, not theirs) |
| Wallet selector (#90)    | Rainbow connect modal            | Chain switch detection + prompt; recent wallets at the top |
| Onboarding (#79)         | Aztec wallet                     | How to onboard users to a custodial-feeling protocol without scaring them |
| Onboarding (secondary)   | Railway Stations                 | Technical onboarding that respects the reader; no patronising copy |
| Empty states (#79)       | Linear                           | Terse, actionable; the empty state itself says what to do next |
| Toasts (#79)             | Linear                           | Top-right anchor, auto-dismiss with hover-to-pause, single primary action |
| Number formatting (F0.5) | TradingView                      | Trailing-zero alignment by decimal place; thousands separators only above 10,000 |

---

## Direct genre peers — what to actually steal

Three products are close enough to what we are building that they
deserve a deeper read. Open them, take notes on *interaction*, ignore
their visual identity.

### Renegade — https://renegade.fi
Direct competitor. Same primitive (privacy-first DEX with MPC matching).
The market-education work is already done.

**Steal:** the *cadence* of their UI. Things move on the protocol's
schedule, not the user's. Their landing page is also useful for copy
that frames the trust model — we can borrow phrasing.

**Skip:** their landing-page motion is marketing; our trading app is a
tool.

### Hyperliquid — https://app.hyperliquid.xyz
Current gold standard for on-chain orderbook trading. Density without
clutter.

**Steal:** orderbook depth-bar implementation, order-ticket density,
positions panel, the multi-stage feedback in their submit button.

**Skip:** their palette (loud reds and greens, semantic). We do not.

### CoW Swap — https://swap.cow.fi
The only mainstream DEX that already had to teach users what a batch
auction is.

**Steal:** their copy explaining batch auctions, MEV protection, and
the trust delta vs a normal DEX. We are explaining the same protocol
property; do not reinvent the words.

**Skip:** the cow / playful brand voice. `DESIGN.md` rules that out.

---

## Tone of copy

`DESIGN.md` Don'ts already says: "no marketing tone, no exclamation
marks, no emoji, no superlatives. DarkPool states facts." Concrete
applications:

| Context              | Write                                   | Don't write |
|----------------------|------------------------------------------|-------------|
| Insufficient balance | `Insufficient balance.`                  | `Oops! It looks like you may not have enough.` |
| Order placed         | `Order placed. Pending next auction.`    | `🎉 Order submitted successfully!` |
| Tx pending           | `Submitting…`                            | `Hang tight while we send your transaction.` |
| Tx rejected by user  | `Rejected by wallet.`                    | `Looks like you cancelled — no worries, try again.` |
| Onboarding intro     | `Orders are encrypted to the operator. Settlement is in batches every 5 s.` | `Welcome to the future of MEV-free trading!` |
| Empty orderbook      | `No orders yet.`                         | `Looks like nobody's trading right now…` |
| Auction settled      | `[ AUCTION 1042 SETTLED · 0.04 ETH @ 2,418.10 USDC ]` | `Great news — your trade just went through! ✨` |

Errors are *informative*, not apologetic. Success is *acknowledged*,
not celebrated. The cadence of the protocol does the celebration.

---

## Patterns to skip (not products)

These are *habits* we avoid, regardless of where we see them.

- **Heavy GSAP entrance choreography on every route change.** GSAP is
  the motion stack of the house (per `DESIGN.md`), but trading routes
  are tools, not stories. Use entrance reveals on first load only;
  internal navigation is instant.
- **"Are you sure?" dialogs for trivial actions.** Cancel order does
  not need a confirmation modal. The cancel button can pause for 800ms
  with an undo affordance — `[ CANCELLED · UNDO ]` toast — which is
  faster, less friction, and more trustworthy.
- **Modal stacking for multi-stage transactions.** Order placement
  has 4 stages (witness → proof → encrypt → submit). Do **not** open
  4 modals. The submit button mutates its own label and shows a thin
  progress bar underneath, all in place.
- **Toast spam for actions the user just performed.** A click is its
  own acknowledgment. Toast only for *deferred* events (auction
  settled in a tab the user isn't focused on, withdrawal confirmed on
  chain).
- **Friendly fintech copy** ("Awesome!", "Heads up!", emoji).
  `DESIGN.md` already rules these out; flagging here because they
  sneak in via copy-paste from other DEX UIs.
- **Sentence-case display.** Bebas Neue is always uppercase. Body is
  always sentence case. Don't mix.
- **Wallet / token logos.** No icons. Bracketed tags only.
- **Native browser confirms / alerts.** Always custom `modal-surface`.
- **Loading spinners.** Box-drawing skeletons. See "no icons" above.

---

## First-pass decisions per panel

All decisions below are expressed in `DESIGN.md` tokens. If you
disagree, push back on the issue **before** merging the PR, not after.

### Layout shell — `/app` (F1.1, #68)
- `nav-bar` baseline. Brand wordmark left (`nav-brand`), then primary
  nav links (`nav-link`): `[ TRADE ]` `[ PORTFOLIO ]` `[ DOCS ]`. Far
  right: chain badge + wallet connect button (`button-ghost` until
  connected, then bracketed-tag with truncated address).
- Right gutter (280px desktop only) reserved for `terminal-feed` —
  surface ambient signal that the engine is alive (stream of:
  encrypted-order arrivals, auction ticks, batch-settled events; all
  redacted as `█ ███████` per the box-drawing-redaction convention).
- Main area: three columns `1fr 2fr 1fr` on `>=1280px`. Orderbook left,
  chart + entry centre, auction tape right. Collapses to single column
  + entry behind a Sheet on `<1024px`.
- Section dividers are 1px `outline` lines. No card chrome between
  panels — the dot-grid overlay shows through.

### Orderbook (#73)
- Two stacked tables (bids top, asks bottom) on narrow screens; side
  by side on wide screens. Decide once, commit, document.
- Column header: bracketed tag `[ BIDS ]` / `[ ASKS ]` in
  `tag-bracketed-static`.
- Row backgrounds carry cumulative-size bars (see bid/ask tension
  section above).
- Click row → autofill order entry (price). Hover row → highlight all
  rows at the same price tier in both bids and asks (subtle —
  `outline-variant` border-bottom only).
- Last-trade row sits between bids and asks: `display-sm` Bebas Neue
  white, centered, with `[ LAST ]` bracketed tag in `secondary` below.

### Auction tape (#74)
- Vertical strip. Each row: timestamp · price · size · count, all in
  `body-sm` IBM Plex Mono.
- New auctions enter at the top with a single soft pulse via
  `status-pill-live` blinking once next to the row.
- Tape header is a `ticker-bar` showing `[ NEXT AUCTION IN 03 · 02 · 01 ]`
  countdown. The countdown text uses `tertiary` (this is where the lime
  accent lives by default on `/app/trade`).
- Selecting a row opens a side drawer with the full auction details +
  Etherscan link (filled in by I2.11 once on-chain).

### Order entry form (#76)
- Vertical card on `card-surface`. Side toggle on top — segmented
  control, both options in `button-ghost` styling, the active one
  switches text to `primary`.
- Inputs use `input-text` + `input-label` above (uppercase tracked).
  Label `[ PRICE · USDC ]`, `[ SIZE · WETH ]`.
- Total + fee preview row below in `body-sm` mono.
- Place button is `button-primary` (lime); when the form is valid the
  accent migrates from the auction countdown to this button. Tape
  countdown drops to `secondary` while the form is the focus.
- Submission: button label mutates through the 4 stages, with a thin
  `tertiary` progress bar underneath the button (height 2px, filling
  left to right). No modals.

### Charts (#75)
- `lightweight-charts` themed: background = `surface`, grid =
  `outline`, text = `secondary`, candle wicks = `primary`. Depth chart
  uses the same `primary` / `secondary` opacity tricks as the
  orderbook bars.
- Time-frame switcher row above the chart: `button-ghost` row,
  `[ 1M ]` `[ 5M ]` `[ 1H ]`. Active one in `primary`.

### Deposit / withdraw modals (#72)
- `modal-surface`. Header: `[ DEPOSIT USDC ]` in `display-sm`.
- Step indicator using `step-node` for `01 APPROVE` → `02 DEPOSIT`.
  The current step uses `tertiary`; completed steps use `primary`;
  pending steps use `secondary`.
- Amount input is `input-text`. MAX shortcut is a `button-ghost` with
  text `[ MAX ]`.
- Confirm button is `button-primary` (lime budget for this surface).

### My orders (#77)
- `table-header` / `table-row` pattern. Columns: time · side · price ·
  size · status · cancel.
- Status: `status-pill-live` blinking for `[ OPEN ]`, static for
  `[ FILLED ]` / `[ CANCELLED ]`.
- Cancel button is `button-ghost` with `[ CANCEL ]` label. After click,
  the row collapses to `[ CANCELLED · UNDO ]` for 5 s before
  disappearing.

### Portfolio (#78)
- Header row of stats: three `stat-value` cells with `stat-label`
  underneath, separated by 1px vertical `divider`s.
  - `WETH POSITION` · `USDC BALANCE` · `UNREALIZED P&L`.
- Lime accent on the P&L value when positive (this surface's accent
  budget).
- Below: fill history as a long `table-row` list with same column
  pattern as #77, plus a settled-tx column with `[ 0x4f3a… ]` linking
  to Etherscan.
- Export CSV is a `button-ghost` in the table header row.

### Onboarding (#79)
- Three modals, navigated via `step-node` row at the top
  (`01 PROTOCOL` · `02 CUSTODY` · `03 PROVING`).
- Body copy in `body-md`, sentence case, no marketing voice (see Tone
  of copy section).
- Each modal has one `button-primary` (`[ NEXT ]` or `[ START TRADING ]`
  on the last step). Dismiss persists per wallet.

---

## When you use this doc

1. Open `DESIGN.md` first. Internalize the tokens.
2. Open the primary reference URL listed for your feature. Take a
   screenshot. Drop it in the PR description as the *interaction*
   target — the visual layer is ours.
3. Build to match the information architecture and density, not the
   exact pixels.
4. Invoke `frontend-design`, `make-interfaces-feel-better`, and
   `web-design-guidelines` skills as appropriate (see `CLAUDE.md`).
5. If you intentionally deviate from a decision in this doc, link back
   to the section in your PR and explain why.
