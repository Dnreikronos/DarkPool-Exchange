# Parallel Work via Git Worktrees

This repo runs multiple Claude Code sessions in parallel against different
issues from the [Trading App MVP epic (#62)][epic]. To make that safe we
use **git worktrees** plus a strict wave plan that keeps independent issues
from colliding on the same files.

[epic]: https://github.com/Dnreikronos/DarkPool-Exchange/issues/62

> **Every Claude session starting work on an issue MUST read this file
> first.** It is referenced from `CLAUDE.md` so the system loads it for
> you. Do not skip the wave plan or the file scope.

---

## TL;DR — kicking off a new agent

```fish
# From the main checkout (/home/mario/DarkPool-Exchange):
set ISSUE 70
set SLUG mock-wallet
git fetch origin
git worktree add ../darkpool-wt/$ISSUE-$SLUG -b feat/issue-$ISSUE-$SLUG origin/main
cd ../darkpool-wt/$ISSUE-$SLUG
gh issue view $ISSUE          # remind yourself of the spec
# launch claude here
```

When the work is done:

```fish
git push -u origin feat/issue-70-mock-wallet
gh pr create --title "F1.3: mock wallet" --body "Closes #70"
# after merge, from the main checkout:
git worktree remove ../darkpool-wt/70-mock-wallet
git branch -D feat/issue-70-mock-wallet      # optional cleanup
```

Worktrees live **outside** the main repo dir (in `../darkpool-wt/`) so
they don't get scanned by `find`, `cargo build` in the parent, IDE
indexers, etc.

---

## Wave plan — what can run in parallel

Each wave must finish (PRs merged to `main`) before the next wave starts.
Inside a wave all issues are independent and can be picked up by different
Claude instances simultaneously.

### Wave 0 — solo (everything depends on this)
| Issue | Title                                                 |
|------ |-------------------------------------------------------|
| #63   | F0.1 Bootstrap trading routes inside front/           |

### Wave 1 — 4 agents in parallel
| Issue | Title                                              | Touches                          |
|------ |----------------------------------------------------|----------------------------------|
| #64   | F0.3 Codegen dp-sdk from proto                     | `front/lib/sdk/`               |
| #66   | F0.5 Design tokens + shadcn + NumericText          | `front/components/ui/`|
| #67   | F0.6 Env config + dev proxy                        | `front/lib/config.ts`     |
| #70   | F1.3 Mock wallet                                   | `front/lib/wallet/`   |
| #89   | C9 ADR + units.ts decimals contract                | `docs/adr/`, `front/lib/units.ts` |

### Wave 2 — 2 agents in parallel
| Issue | Title                                              | Depends on |
|------ |----------------------------------------------------|------------|
| #65   | F0.4 DarkPoolClient interface + Mock/Rest impls    | #64        |
| #68   | F1.1 Layout shell `/trade`                         | #66        |

### Wave 3 — solo (everything from Phase 1 reads this)
| Issue | Title                                              | Depends on |
|------ |----------------------------------------------------|------------|
| #69   | F1.2 Mock store + faker factories                  | #64, #65   |

### Wave 4 — 3 agents in parallel
| Issue | Title                                              | Depends on |
|------ |----------------------------------------------------|------------|
| #71   | F1.4 Balances panel                                | #69, #70   |
| #73   | F1.6 Orderbook view                                | #69        |
| #74   | F1.7 Auction tape                                  | #69        |

### Wave 5 — 3 agents in parallel
| Issue | Title                                              | Depends on |
|------ |----------------------------------------------------|------------|
| #72   | F1.5 Deposit/withdraw modals                       | #71        |
| #75   | F1.8 Depth + price charts                          | #73, #74   |
| #76   | F1.9 Order entry form                              | #71, #73   |

### Wave 6 — 2 agents in parallel
| Issue | Title                                              | Depends on |
|------ |----------------------------------------------------|------------|
| #77   | F1.10 My orders + cancel                           | #76        |
| #78   | F1.11 Portfolio + P&L                              | #77 (or run in parallel; #78 reads from store) |

### Wave 7 — polish (run after every panel above is on main)
| Issue | Title                                              | Notes |
|------ |----------------------------------------------------|-------|
| #79   | F1.12 Onboarding + empty/error/skeleton            | Touches every panel — DO NOT run in parallel with Waves 4–6 |
| #80   | F1.13 A11y + reduced-motion + responsive           | After #79 |

### Cross-cutting backend / contracts — parallel with ANY frontend wave
These touch `crates/` and `contracts/` — zero overlap with the frontend
worktrees, so they can run at any time on their own worktrees.

| Issue | Title                                              | Touches             |
|------ |----------------------------------------------------|---------------------|
| #81   | C1 GET /v1/operator/pubkey                         | `crates/dp-api/src/rest.rs`, `crates/dp-crypto/` |
| #82   | C2 CORS on REST router                             | `crates/dp-api/src/rest.rs` |
| #83   | C3 tonic-web or SSE bridge                         | `crates/dp-api/src/main.rs` |
| #84   | C4 Publish contracts/deployments/{chainId}.json    | `contracts/script/` |
| #85   | C5 ADR per-order vs batch proof                    | `docs/adr/`         |
| #86   | C6 dp-zk-cli commit + prove-single-order           | `crates/dp-zk-cli/` |
| #87   | C7 x-request-id middleware                         | `crates/dp-api/src/` |
| #88   | C8 docker-compose for local stack                  | `docker-compose.yml`, `Dockerfile` |

⚠ **C1, C2, C7 all touch `crates/dp-api/src/rest.rs` or `main.rs`.**
Pick one at a time per file or coordinate to take the same worktree.

### Phase 2 — integration (after Phase 1 and the relevant Cs)

| Issue | Title                                              | Hard prerequisites |
|------ |----------------------------------------------------|--------------------|
| #90   | I2.1 Real wallet (RainbowKit)                      | #70                |
| #91   | I2.2 wagmi generate from ABIs                      | #90, #84 (C4)      |
| #92   | I2.3 On-chain deposit/withdraw                     | #91                |
| #93   | I2.4 Real orderbook REST                           | #82 (C2)           |
| #94   | I2.5 Real tape REST                                | #82 (C2)           |
| #95   | I2.6 Streaming upgrade                             | #83 (C3), #94      |
| #96   | I2.7 Browser ECIES                                 | #81 (C1)           |
| #97   | I2.8 WASM commit                                   | #85 (C5), #86 (C6) |
| #98   | I2.9 WASM prover                                   | #97                |
| #99   | I2.10 Real order placement e2e                     | #96, #98, #87 (C7) |
| #100  | I2.11 BatchSettled linkage                         | #92, #94           |
| #101  | I2.12 IndexedDB history                            | #99                |
| #102  | I2.13 Production deploy                            | #99, #100, #88 (C8) |

Within Phase 2 most issues swap a single mock for the real client and so
touch a small, well-isolated set of files. Multiple Phase 2 issues can
run in parallel **if their dependencies are merged**.

---

## File scope per issue

The scopes below are the **only** paths an agent should add or edit while
working on the issue. If a panel needs a new shared component, **flag it
in the PR and stop**; do not edit a sibling panel's directory.

> ⚠ **Path note.** Many issue bodies were written assuming a separate
> `apps/trading/` workspace. We later decided to keep everything inside
> the existing `front/` Next app (landing at `/`, trading at `/app`).
> **Translate any `apps/trading/src/...` reference in an issue body to
> `front/...` per the layout below.**

Shared scaffolding (only F0.1, F0.3, F0.4, F0.5, F0.6 should touch these):

- `front/package.json`, `front/tailwind.config.ts`, `front/tsconfig.json`,
  `front/next.config.mjs` — existing files; **additive edits only**
- `.github/workflows/` (CI hooks)

The landing surface is **off-limits** to trading-app issues:

- `front/app/page.tsx`, `front/app/layout.tsx`, `front/app/globals.css`
- `front/components/{Footer,Hero,HowItWorks,Nav,OrderFlowPanel,TerminalFeed,Ticker}.tsx`
- `front/lib/shaders/`

If the trading work needs to touch any landing file, **stop and open a
PR comment on the issue**.

After Phase 0 lands, the trading app lives under `front/app/app/`:

```
front/
├── app/
│   ├── layout.tsx                       ← landing baseline; do NOT edit from trading issues
│   ├── page.tsx                         ← landing route /; off-limits
│   ├── globals.css                      ← shared; only F0.5 may extend
│   └── app/                             ← trading routes under /app
│       ├── layout.tsx                   ← F1.1 only
│       ├── page.tsx                     ← F1.1 (redirect to /app/trade or compose Shell)
│       ├── trade/page.tsx               ← F1.1 baseline; only adds slot composition
│       └── portfolio/page.tsx           ← F1.11 only
├── components/
│   ├── ui/                              ← F0.5 baseline; new primitives only via PR
│   ├── NumericText.tsx                  ← F0.5
│   └── trade/                           ← all trading-app components here
│       ├── Shell.tsx                    ← F1.1
│       ├── ConnectButton.tsx            ← F1.3 (mock) and #90 (real)
│       ├── balances/                    ← F1.4 + #91/#92 swap internals
│       ├── deposit/                     ← F1.5 + #92 swap internals
│       ├── orderbook/                   ← F1.6 + #93 swap internals
│       ├── tape/                        ← F1.7 + #94/#95 swap internals
│       ├── charts/                      ← F1.8
│       ├── entry/                       ← F1.9 + #99 swap internals
│       └── my-orders/                   ← F1.10
└── lib/
    ├── sdk/
    │   ├── proto/                       ← F0.3 (generated, do not hand-edit)
    │   ├── client.ts                    ← F0.4
    │   ├── mocks/                       ← F1.2
    │   └── index.ts                     ← all owners append exports
    ├── api-client.ts                    ← F0.4 baseline (re-exports from sdk)
    ├── wallet/                          ← F1.3 baseline; #90 replaces internals
    ├── mock-store.ts                    ← F1.2 only
    ├── ecies.ts                         ← #96 only
    ├── prover/                          ← #97/#98 only
    ├── units.ts                         ← C9 / #89 only
    └── config.ts                        ← F0.6 only
```

For ZK WASM (Rust side):

```
crates/dp-zk-wasm/                       ← #97 creates; #98 extends
```

If you find yourself wanting to edit a file outside your scope, **stop
and open a PR comment on the issue** so the user can decide whether to
merge it into your scope or spin a new issue.

---

## Conventions

### Branching
- `feat/issue-<number>-<short-slug>` — one branch per issue.
- Branch from `origin/main` only. No long-lived `feat/frontend-mvp`
  integration branch; we ship straight to `main`.

### PRs
- One PR per issue. PR body opens with `Closes #<issue>` so the issue
  autocloses on merge.
- Title format: `[<issue-tag>] <short title>` e.g. `[F1.6] Orderbook view`.
- Keep PRs small (target ≤ 500 LoC where possible). If an issue is too
  big, split it and add new issues — do not bundle.

### Keeping the worktree fresh
Before pushing, rebase on top of fresh `main`:

```fish
cd ../darkpool-wt/$ISSUE-$SLUG
git fetch origin
git rebase origin/main
```

If a rebase hits a conflict outside your declared file scope, that's a
signal someone else broke the scope — stop and surface it to the user.

### Package.json conflicts
Every dep-adding issue will touch `front/package.json`. These are
trivially auto-mergeable when isolated (additions to different dependency
sections). If conflicts appear they almost always mean two agents added
the same dep with different versions — pick the higher version and move
on.

### Lockfile
Use `pnpm-lock.yaml` (or `package-lock.json`, depending on what F0.1
chooses) — **don't commit lockfile changes from a partial install**.
Always run a full install on the worktree before committing.

---

## When something goes wrong

| Symptom                                          | Action |
|--------------------------------------------------|--------|
| Rebase conflict outside your file scope          | Stop. Comment on the issue with the conflicting paths. Wait for guidance. |
| Two issues need the same new component           | Open it as a tiny prerequisite PR in `front/components/ui/` so both can rebase on it. |
| Your wave isn't unblocked yet (deps not merged)  | Don't start. Pick a different issue from the current open wave, or a cross-cutting backend one. |
| Worktree got corrupted                           | `git worktree remove --force ../darkpool-wt/<n>-<slug>` and re-create. |

---

## How to dispatch a Claude session

Brief prompt template for each spawned Claude:

> You are working on issue #<N> of [the trading-app MVP epic][epic] in a
> dedicated git worktree at `<path>`. Read
> `docs/PARALLEL-WORK.md` first. Your file scope is what that doc lists
> for this issue; do not edit anything outside it. Read the full issue
> body via `gh issue view <N>` and follow its acceptance criteria
> exactly. When done, open a PR with `Closes #<N>` in the body.

[epic]: https://github.com/Dnreikronos/DarkPool-Exchange/issues/62
