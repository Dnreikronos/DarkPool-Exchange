# [I2.2] Real contract reads (wagmi-generated ABIs + multicall balances)

**Issue:** [#91](https://github.com/Dnreikronos/DarkPool-Exchange/issues/91) · **Epic:** #62
**Date:** 2026-05-29 · **Base branch:** `main` · **Branch:** `feat/issue-91-contract-reads`
**Complexity:** M

---

## Why

The balances panel (#73/#113, F1.4) is currently mock-backed. We already commit
the source-of-truth ABIs at `crates/dp-settlement/abi/DarkPool.json` and
`VerifierProxy.json`. Hand-written TypeScript ABIs would drift from those. This
PR generates typed ABIs from the committed JSON, reads real on-chain balances
through them, and adds a CI drift test — analogous to the existing Rust
`crates/dp-settlement/tests/abi_drift.rs`.

## Current state on `main` (verified in the worktree at `b4f4a41`)

`origin/main` already contains the full Phase-2 wallet stack (I2.1 / #127):

- `wagmi ^2.19.5` + `@rainbow-me/rainbowkit` are dependencies.
- `front/lib/wallet/WalletProviders.tsx` wraps the trading app in
  `WagmiProvider → QueryClientProvider → RainbowKitProvider`, with a single
  shared `QueryClient`.
- `front/lib/wallet/WagmiWalletBridge.tsx` drives `walletStore` from wagmi's
  `useAccount`, so `useWallet().address` / `useTraderId()` reflect the **real**
  connected address in production and a synchronous mock in tests/stories.
- `front/lib/wallet/wagmi-config.ts` resolves `targetChain` from
  `NEXT_PUBLIC_CHAIN_ID` and builds the wagmi config via RainbowKit's
  `getDefaultConfig` — which also configures the chain **transport**. No
  separate RPC URL or viem public client is needed.

Only `useWalletBalances()` / `useInternalBalances()` are still mock (they read
`walletStore`). Those are exactly what this PR replaces with on-chain reads.

> Note: an earlier draft of this spec assumed `main` had no `wagmi` (it was
> written against a stale local `main`). That assumption was wrong; this spec
> supersedes it and uses wagmi's own hooks directly.

## Approach (wagmi-native, literal to the acceptance criteria)

`@wagmi/cli` runs as a **dev-only** dependency and generates typed `as const`
ABI arrays from the two JSON files using its **core** behaviour (no `react`
plugin) — output is `darkPoolAbi` / `verifierProxyAbi`. Balances are read with
wagmi's generic **`useReadContracts`** (a single multicall) and refreshed by
**`useWatchContractEvent`** on `Deposit` / `Withdrawal` — exactly the hooks the
issue names. ERC-20 wallet balances use viem's built-in `erc20Abi` (no
generation needed). The reads run on the **shared** `QueryClient` from
`WalletProviders`, so the existing `WagmiWalletBridge` clears them on account
switch/disconnect for free.

Why core plugin (typed ABI consts) rather than the `react` plugin (generated
per-contract hooks): the generic `useReadContracts` / `useWatchContractEvent`
calls with the generated `abi` satisfy the acceptance criteria with far less
generated churn, and keep the drift test trivial (compare one exported const to
one JSON file).

---

## Architecture & file scope

```
front/
├── wagmi.config.ts                        NEW   @wagmi/cli config → ABI consts only
├── package.json                           EDIT  +@wagmi/cli (devDep); "codegen" script; npm-10 lockfile
├── .env.local.example                     EDIT  +NEXT_PUBLIC_USE_MOCKS_BALANCES (documented)
├── lib/contracts/
│   ├── generated.ts                       NEW (generated)  darkPoolAbi / verifierProxyAbi
│   └── generated.test.ts                  NEW   ABI-drift test (TS const ↔ crates/.../*.json)
└── app/app/trade/
    ├── _hooks/balances/
    │   ├── useChainBalances.ts            NEW   wagmi useReadContracts + useWatchContractEvent
    │   ├── useChainBalances.test.ts       NEW
    │   ├── useBalances.ts                  NEW   selector: chain vs mock → unified shape
    │   └── useBalances.test.ts            NEW
    └── _components/balances/
        ├── BalancesPanel.tsx              EDIT  consume useBalances(); wire loading/error states
        ├── states.tsx                     EDIT  +BalancesLoading, +BalancesError (next to BalancesDisconnected)
        └── BalancesPanel.test.tsx         EDIT  cover mock / chain / loading / error / disconnected
```

Co-location (CLAUDE.md): route-scoped hooks under
`app/app/trade/_hooks/balances/`; the generated ABIs + drift test are shared
infrastructure, so they live under `front/lib/contracts/` alongside
`lib/config.ts` / `lib/units.ts`.

`config.ts` is **not** modified — `config.contracts` (the existing discriminated
union: addresses present only when `NEXT_PUBLIC_USE_MOCKS=false`) already
supplies the addresses, and wagmi-config already supplies the transport.

### Codegen pipeline

- `wagmi.config.ts`: `@wagmi/cli` default export, **no plugins**, one
  `contracts` entry per ABI with `abi` set to the parsed JSON
  (`crates/dp-settlement/abi/DarkPool.json`, `VerifierProxy.json`), and
  `out: 'lib/contracts/generated.ts'`. Output is two
  `export const …Abi = [...] as const` arrays — typed via `abitype`,
  importing nothing from `wagmi`.
- `package.json`: `"codegen": "npm run sdk:gen && wagmi generate"` so the proto
  SDK (`buf generate`) and the ABI consts regenerate together (satisfies "wires
  this together with F0.3 codegen"). `@wagmi/cli` added to `devDependencies`.
- After the `package.json` change, regenerate the lockfile with **npm 10** (CI
  runs npm 10.8.2; an npm-11 lockfile fails `npm ci`).

## Data flow

1. `config` provides `chainId`, `contracts.{darkPool,weth,usdc}`, `useMocks`.
2. `useBalances()` decides the source: mocks on → existing
   `useWalletBalances` / `useInternalBalances`; mocks off → `useChainBalances`.
   The toggle is `NEXT_PUBLIC_USE_MOCKS_BALANCES`, falling back to the global
   `NEXT_PUBLIC_USE_MOCKS`. It is read via **direct
   `process.env.NEXT_PUBLIC_USE_MOCKS_BALANCES` access** (not bracket access) so
   Next's static `NEXT_PUBLIC_*` inlining still works — the same constraint
   documented in `lib/sdk/client.ts`. Because balances are an on-chain read, not
   an SDK method, this lives in the route layer and does **not** extend
   `METHOD_ENV_VAR`.
3. `useChainBalances(trader)`:
   - `useReadContracts` with `allowFailure: false` and four calls in one
     multicall:
     - `darkPool.balances(trader, weth)` → `[ DARKPOOL ]` WETH
     - `darkPool.balances(trader, usdc)` → `[ DARKPOOL ]` USDC
     - `weth.balanceOf(trader)` (viem `erc20Abi`) → `[ WALLET ]` WETH
     - `usdc.balanceOf(trader)` (viem `erc20Abi`) → `[ WALLET ]` USDC
   - `query: { enabled: !useMocks && Boolean(trader) }` (hooks are always
     called; the query is disabled when there is nothing to read).
   - `useWatchContractEvent` on `Deposit` and `Withdrawal` (filtered to
     `trader`) → `refetch()`.
   - Each returned `bigint` is formatted with `formatUnits` + `TOKEN_DECIMALS`
     from `lib/units.ts` into the decimal-**string** `{ weth, usdc }` shape the
     panel already renders. **Never `Number()` / `parseFloat`.**
   - Returns `{ wallet, internal, isLoading, isError, refetch }`.

Because `useReadContracts` uses the nearest `QueryClient` — the shared one from
`WalletProviders` — these queries are cleared by `WagmiWalletBridge` on
disconnect/account-switch automatically. (Unlike `OrderBook`, which owns a
separate client; balances deliberately stay on the shared client.)

## Error & edge handling

- **RPC down / multicall revert** → `isError` → panel shows a compact
  `[ READ FAILED ]` row with retry (`refetch`), reusing the onboarding
  error-state styling (#79). Note the shared `QueryClient` sets `retry: false`,
  so failures surface immediately rather than after retries.
- **Loading** → skeleton rows in the two columns (reuse #79 skeleton styling).
- **`useMocks=true`** (default, incl. CI) → panel renders mock data exactly as
  today; live reads and placeholder zero-addresses are never dialed.
- **No connected trader** → existing `BalancesDisconnected` state.
- **Live mode against zero/placeholder addresses**: documented as requiring real
  deployed addresses (C4 / #84). Not fixed here.

## Testing (TDD, red → green → refactor)

- `generated.test.ts` — **drift**: deep-compare each exported ABI const to its
  source JSON (`crates/dp-settlement/abi/{DarkPool,VerifierProxy}.json`),
  normalizing top-level order the way the Rust test does. Failure message:
  "ABI drift — run `npm run codegen`". Runs under `npm test`, already in the
  frontend CI lane (#107) — **no CI YAML change**. JSON files are reachable from
  `front/` at `../crates/dp-settlement/abi/*.json` in the monorepo checkout.
- `useChainBalances.test.ts` — mock `wagmi` (`useReadContracts`,
  `useWatchContractEvent`); assert: multicall request shape (4 calls, right
  addresses/args), decimal-string formatting (6-dp USDC vs 18-dp WETH), and that
  a simulated `Deposit`/`Withdrawal` event triggers `refetch`. `enabled` is
  false under mocks / no trader.
- `useBalances.test.ts` — toggles `NEXT_PUBLIC_USE_MOCKS_BALANCES` /
  `NEXT_PUBLIC_USE_MOCKS`; asserts it selects the mock hooks vs chain hook.
- `BalancesPanel.test.tsx` — extend to cover mock / chain / loading / error /
  disconnected states (testing-library; wrap chain cases in a
  `QueryClientProvider`).

## Out of scope (YAGNI)

Deposit/withdraw **writes** (#92), allowance reads, multi-pair, SIWE, the
`react`-plugin generated hooks, and any changes to `wagmi-config.ts`,
`WalletProviders`, or RainbowKit. `config.ts` is untouched.

## Acceptance criteria mapping

| Issue criterion | How this PR meets it |
|---|---|
| `wagmi.config.ts` consumes the two ABI JSONs | `wagmi.config.ts`, no plugins, `abi` from parsed JSON |
| `npm run codegen` wires with F0.3 codegen | `"codegen": "npm run sdk:gen && wagmi generate"` |
| F1.4 panel calls `balances(trader, token)` via multicall + watches Deposit/Withdrawal with `useWatchContractEvent` | `useChainBalances`: `useReadContracts` multicall + `useWatchContractEvent` (the literal hooks) |
| ABI-drift test fails CI on out-of-sync output | `generated.test.ts` under `npm test` |

## PR meta

- Branch off `origin/main`, via git worktree.
- Title: `[I2.2] Real contract reads (wagmi-generated ABIs + multicall balances)`.
- Body opens with `Closes #91`.
- One PR, no direct commits to `main`, no Claude co-author trailer.
