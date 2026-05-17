# Claude session orientation

This repository is being built with **multiple Claude Code sessions
running in parallel**, each in its own git worktree, each tackling a
different GitHub issue from the trading-app MVP epic
([#62](https://github.com/Dnreikronos/DarkPool-Exchange/issues/62)).

---

## Before you do anything else

1. **Read `docs/PARALLEL-WORK.md`.** It contains the wave plan, the
   file-scope rules per issue, the git-worktree recipe, and the PR
   conventions. Working without reading it will collide with another
   agent's work.
2. **If your issue is UI work, read `DESIGN.md` (root) first, then
   `docs/DESIGN-INSPIRATIONS.md`.** `DESIGN.md` is the canonical design
   system — brutalist trading-terminal aesthetic, lime accent (`#D4FF00`)
   used **at most once per view**, Bebas Neue + IBM Plex Mono, zero
   border radius, no semantic colors (no green/red), no icons, GSAP as
   the motion stack. `DESIGN-INSPIRATIONS.md` is the complement:
   per-feature UX references (Hyperliquid, Renegade, CoW, Phantom for
   patterns — not for visual identity), trading-specific tensions
   resolved (bid/ask without hue, accent budget per surface), tone of
   copy, and per-panel first-pass decisions all expressed in
   `DESIGN.md` tokens.
3. **Confirm which issue you are working on.** If unsure, ask the user.
   Then read the issue body with `gh issue view <N>` for the full
   acceptance criteria.
4. **Confirm you are in the right worktree.** Run `git rev-parse --show-toplevel`
   and `git branch --show-current`. If you are in the main checkout
   (`/home/mario/DarkPool-Exchange` on branch `main`) and starting new
   work, stop and create a worktree per the recipe in
   `docs/PARALLEL-WORK.md`.
5. **Invoke the relevant skills.** See the *Recommended skills* section
   below — there is almost always a skill that applies.

---

## What this project is

**ZK Dark Pool DEX.** A decentralized exchange where orders stay
private until settlement. Traders encrypt their orders to the engine
operator's public key and submit a ZK proof of validity. The operator
matches orders in periodic batch auctions (5 s default) and proves the
matching was executed correctly. Settlement is on-chain with an
aggregated Groth16 proof.

**Why it matters.** On normal DEXs orders sit in a public mempool where
MEV bots front-run, sandwich, and extract value. This protocol keeps
orders invisible to external observers before settlement while still
producing a trustless clearing price per auction round.

**Trust model.** Semi-trusted operator + ZK proof of correct execution.
Mirrors institutional dark pools in TradFi (regulated venue operator)
but cryptographically binds the operator to fair execution.

**Target user.** Hedge funds, market makers and DeFi protocols that
need MEV protection. Institutional-grade, not retail.

Read `README.md`, `overview.md`, and `architecture-future-decisions.md`
in the repo root for the longer story.

---

## Stack & structure

### Rust workspace (`crates/`) — backend, fully functional
- `dp-api` — REST (`POST /v1/orders`, `GET /v1/orderbook`, etc.) + gRPC
  + streaming `StreamAuctions`. Auth via `x-api-key`.
  Proto: `crates/dp-api/proto/darkpool/v1/darkpool.proto` (source of truth
  for the TypeScript SDK).
- `dp-engine` — order matching engine, in-memory order book, periodic
  auction tick, batch building.
- `dp-auction` — clearing-price computation.
- `dp-settlement` — submits batches to chain via `alloy`. ABIs in
  `crates/dp-settlement/abi/`.
- `dp-crypto` — ECIES decryption (operator-side).
- `dp-zk` + `dp-zk-cli` — arkworks Groth16 over BN254 with Poseidon
  commitments. Native today; WASM build planned (#97, #98).
- `dp-event` — event-sourced store (in-memory / file / Postgres).
- `dp-types`, `dp-book`, `dp-aggregator`, `dp-client` — supporting
  crates.

### Solidity contracts (`contracts/`) — deployed pattern, no fixed addresses yet
- `DarkPool.sol` — escrow + `submitBatch`. Trader deposits ERC20s,
  operator submits batches with Groth16 proof, contract settles by
  shuffling internal balances. Events: `Deposit`, `Withdrawal`,
  `BatchSettled`. Protocol fee 5 bps. **No EIP-712, no Permit2.**
- `VerifierProxy.sol` — governance router for the verifier; allows
  rotating the verifying key without redeploying `DarkPool`.
- `Groth16Verifier.sol` — BN254 proof verifier, VK immutable at deploy.
- `contracts/script/Deploy.s.sol` — Foundry deploy script (Cancun EVM,
  via-IR, 200 optimizer runs).

### Frontend (`front/`) — single Next.js app, landing + trading
- `front/` — Next.js 14.2 App Router, TypeScript, Tailwind v3.4,
  React 18. **Both the marketing landing AND the trading app live
  here.**
  - `front/app/page.tsx` — landing (`/`), already shipping at
    https://front-five-flax.vercel.app. Built with GSAP + custom
    shaders. Don't break it.
  - `front/app/app/...` — trading app (`/app/...`), being built from
    the epic. New routes go under this segment.
- The deps for the trading app (wagmi, viem, RainbowKit, TanStack
  Query, Zustand, shadcn primitives, decimal.js, etc.) get added to
  the existing `front/package.json` — there is **no `apps/trading/`
  monorepo split**. F0.1 (#63) does this in place.
- The TypeScript SDK generated from the proto lives at `front/lib/sdk/`
  (NOT `packages/dp-sdk/`).
- File-path note for issue bodies: many issues were written when the
  plan was a separate `apps/trading/` workspace. **Read any reference
  to `apps/trading/src/...` as `front/...`.** The epic carries the
  mapping table.

### Locked-in product decisions
- **MVP scope:** trading app only. Landing page is out of scope.
- **Pairs:** single-pair `ETH/USDC` hardcoded. Multi-pair is gated on
  backend issue #29 (pair registry).
- **ZK proof generation:** in-browser via WASM (not a backend prover
  service). Real concern: prove time of 5–30 s; UX must accommodate.
- **Mock-first strategy:** build the entire UI against typed mocks
  (Phase 1), then swap each mock for real integration (Phase 2). See
  the epic for the full breakdown.
- **Wallet auth on backend = none** — server uses an API key. Per-user
  auth (Sign-In-with-Ethereum) is a post-MVP follow-up.

---

## Hard rules

- **Stay inside your file scope.** The scope per issue is listed in
  `docs/PARALLEL-WORK.md`. If you need to edit a file outside it, stop
  and surface the conflict on the issue.
- **One PR per issue.** Title format `[<tag>] <short title>` (e.g.
  `[F1.6] Orderbook view`). PR body opens with `Closes #<issue>` so
  the issue autocloses on merge.
- **Never commit directly to `main`.** Branch from `origin/main`, push
  the feature branch, open a PR.
- **Never run destructive git commands** (`reset --hard`, `push --force`,
  `branch -D`, `worktree remove --force`) without an explicit user
  request.
- **Numeric wire fields are strings.** `price`, `size`, `clearingPrice`,
  `matchedVolume` come from the API as decimal strings. Never coerce
  to JS `number`. Math via `decimal.js`. On-chain amounts via viem
  `parseUnits` / `formatUnits`.
- **No backwards-compat hacks.** This is a greenfield rewrite. Don't
  add fallbacks or feature flags for the old `front/` — it's gone.
- **Lockfile sanity:** if `package.json` changed, run the full install
  before committing the lockfile.

---

## Recommended skills

Skills override default behavior — invoke them via the `Skill` tool
**before** acting. The rule of thumb: *if you think there's a 1% chance
a skill applies, invoke it.* The list below is a starting point, not
exhaustive.

### Process (almost always applicable)

- **`superpowers:using-git-worktrees`** — at the start of any new
  feature. Ensures you are in an isolated workspace, not on `main`.
- **`superpowers:brainstorming`** — before any creative work (a new
  panel, a new component, a fresh feature). Explores intent before
  implementation.
- **`superpowers:test-driven-development`** — when implementing
  features or bugfixes. Red-green-refactor.
- **`superpowers:systematic-debugging`** — when you hit any bug, test
  failure, or unexpected behavior, *before* proposing a fix.
- **`superpowers:verification-before-completion`** — before claiming
  work is complete, fixed, or passing. Evidence before assertions.
- **`superpowers:requesting-code-review`** — before opening the PR.
- **`karpathy-guidelines`** — code-quality hygiene. Surgical changes,
  surface assumptions, define verifiable success criteria. Especially
  useful when tempted to over-engineer.

### Frontend / UI (Phase 1 panels, Phase 2 polish)

- **`frontend-design`** — when building a new panel or page. Produces
  distinctive, production-grade UI; avoids generic AI aesthetics.
- **`web-design-guidelines`** — for accessibility and UX review. F1.12
  and F1.13 explicitly reference this; use it on every panel PR.
- **`make-interfaces-feel-better`** — for the polish details:
  hover states, shadows, borders, typography, micro-interactions,
  tabular numbers, optical alignment. Especially relevant for the
  orderbook (#73), tape (#74), and order-entry form (#76).
- **`vercel-react-best-practices`** — when writing or reviewing
  React/Next.js code. Performance patterns, RSC vs client components,
  data fetching with TanStack Query.
- **`emil-design-eng`** — UI polish philosophy. Animations, component
  design, the invisible details that make software feel great.
- **`vercel-react-view-transitions`** — if you reach for page or shared-
  element animations. Native React API, no third-party lib.

### Specialized review / polish

- **`polish`** — final quality pass: alignment, spacing, consistency,
  micro-details. Use before marking F1.x complete.
- **`audit`** — accessibility, performance, theming, responsive,
  anti-patterns. F1.13 should invoke this.
- **`critique`** — UX design critique with quantitative scoring.
  Useful on F1.12 onboarding and the portfolio page (#78).
- **`animate`** — when adding motion to the tape (#74) or the
  multi-stage order-entry feedback (#76).
- **`clarify`** — for UX copy. Error messages from #92 (deposit/withdraw
  reverts) and #99 (order rejection) benefit from this.
- **`onboard`** — directly applicable to F1.12 onboarding (#79).

### Architecture decisions

- **`first-principles`** — for the ADRs (#85 per-order vs batch proof,
  #89 decimals). Questions the existing patterns instead of cargo-
  culting them.
- **`diagnose`** — for hard bugs and perf regressions. Reproduce →
  minimise → hypothesise → instrument → fix → regression-test.

### What to skip

- `claude-api`, `claude-code-guide` — we are not building on top of the
  Claude API or modifying Claude Code itself.
- `gsap-*` — the old frontend used GSAP heavily; the new build does
  not. Stick with CSS / Framer Motion / View Transitions API.
- `golang-pro` — backend is Rust, not Go.

---

## Useful commands

```bash
# See the epic checklist
gh issue view 62

# See a specific issue
gh issue view <N>

# List open frontend issues
gh issue list --label frontend --state open

# See which issues are unblocked right now — read docs/PARALLEL-WORK.md
```

```fish
# Start work on a new issue (in fish):
set ISSUE 70
set SLUG mock-wallet
git fetch origin
git worktree add ../darkpool-wt/$ISSUE-$SLUG -b feat/issue-$ISSUE-$SLUG origin/main
cd ../darkpool-wt/$ISSUE-$SLUG
gh issue view $ISSUE
# launch Claude here — it will load this CLAUDE.md and route through PARALLEL-WORK.md
```
