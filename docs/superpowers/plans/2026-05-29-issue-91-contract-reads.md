# [I2.2] Real contract reads — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the mock balances panel with real on-chain reads — `DarkPool.balances(trader, token)` for the internal column and ERC-20 `balanceOf` for the wallet column — driven by `@wagmi/cli`-generated typed ABIs, refreshed on `Deposit`/`Withdrawal` events, with a CI drift test.

**Architecture:** `@wagmi/cli` (dev-only, core/no-plugin) generates `darkPoolAbi`/`verifierProxyAbi` consts from the committed `crates/dp-settlement/abi/*.json`. A pure helper builds the multicall request and maps results to decimal strings. A `useChainBalances` hook wires wagmi's `useReadContracts` + `useWatchContractEvent`. A `useBalances` selector switches mock↔chain on a config toggle. `BalancesPanel` renders by status. `main` already has the full wagmi/RainbowKit provider tree, so no provider or `config.ts` changes are needed.

**Tech Stack:** Next.js 14 / React 18, wagmi v2 + viem, TanStack Query, `@wagmi/cli`, Vitest (+ `@testing-library/react` under jsdom for hooks), Zod-validated `lib/config.ts`.

**Conventions for every commit in this plan:** the user asked that commits be dated 2026-05-29. Prefix each commit with the date env so author **and** committer dates match:

```bash
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "<message>" --date="2026-05-29T12:00:00"
```

Do **not** add any `Co-Authored-By: Claude` trailer (CLAUDE.md hard rule). All commands run from the worktree root `/home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads`; frontend commands run from `front/`.

---

## Pre-flight (run once before Task 1)

- [ ] **Confirm npm 10 and clean baseline**

```bash
node -v && npm -v          # npm MUST be 10.x — an npm-11 lockfile fails `npm ci` in CI
cd front && npm install && npm test
```

Expected: npm reports `10.x`; install succeeds; the existing suite passes (0 failures). If npm is 11.x, switch to npm 10 before touching the lockfile.

---

## Task 1: ABI codegen pipeline (`@wagmi/cli` → typed ABI consts)

**Files:**
- Create: `front/wagmi.config.ts`
- Create (generated): `front/lib/contracts/generated.ts`
- Modify: `front/package.json` (devDep `@wagmi/cli`, `codegen` script)
- Modify: `front/package-lock.json` (npm 10 regeneration)

- [ ] **Step 1: Add `@wagmi/cli` as a dev dependency (npm 10)**

```bash
cd front && npm install --save-dev @wagmi/cli@^2.1.0
```

Expected: `@wagmi/cli` appears in `devDependencies`; `package-lock.json` updates. Confirm with `npm pkg get devDependencies.@wagmi/cli`.

- [ ] **Step 2: Add the `codegen` script**

Edit `front/package.json` `scripts`, adding `codegen` next to the existing `sdk:gen`:

```json
    "sdk:gen": "cd lib/sdk && buf generate",
    "codegen": "npm run sdk:gen && wagmi generate"
```

- [ ] **Step 3: Write `front/wagmi.config.ts`**

The ABI JSON lives outside `front/`, so read it at runtime with `fs` (avoids cross-root JSON `import` problems under `tsc`). Paths resolve from the CWD where `wagmi generate` runs (`front/`).

```ts
import { readFileSync } from 'node:fs'
import { defineConfig } from '@wagmi/cli'
import type { Abi } from 'viem'

// Source-of-truth ABIs committed under crates/dp-settlement/abi. Read at
// config-eval time so the generated file always mirrors them exactly —
// the drift test (generated.test.ts) fails CI if they diverge.
function loadAbi(path: string): Abi {
  return JSON.parse(readFileSync(path, 'utf8')) as Abi
}

export default defineConfig({
  out: 'lib/contracts/generated.ts',
  contracts: [
    { name: 'darkPool', abi: loadAbi('../crates/dp-settlement/abi/DarkPool.json') },
    { name: 'verifierProxy', abi: loadAbi('../crates/dp-settlement/abi/VerifierProxy.json') },
  ],
})
```

- [ ] **Step 4: Generate the ABI consts**

```bash
cd front && npm run codegen && npx prettier --write lib/contracts/generated.ts
```

Expected: `front/lib/contracts/generated.ts` is created containing
`export const darkPoolAbi = [...] as const` and
`export const verifierProxyAbi = [...] as const`.

- [ ] **Step 5: Verify the exports and that types compile**

```bash
cd front && grep -E "export const (darkPoolAbi|verifierProxyAbi)" lib/contracts/generated.ts && npm run typecheck
```

Expected: both export lines print; `tsc --noEmit` exits 0.

- [ ] **Step 6: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/wagmi.config.ts front/lib/contracts/generated.ts front/package.json front/package-lock.json
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "feat(front): generate typed contract ABIs via @wagmi/cli" --date="2026-05-29T12:00:00"
```

---

## Task 2: ABI-drift test (CI gate)

**Files:**
- Create: `front/lib/contracts/generated.test.ts`

- [ ] **Step 1: Write the failing drift test**

`generated.test.ts` sits at `front/lib/contracts/`; the source ABIs are three levels up under `crates/`. Normalize the generated `as const` arrays to plain JSON before comparing. Mirrors `crates/dp-settlement/tests/abi_drift.rs`.

```ts
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, it, expect } from 'vitest'

import { darkPoolAbi, verifierProxyAbi } from './generated'

// front/lib/contracts -> ../../../ = repo root, then crates/dp-settlement/abi.
function loadSourceAbi(contract: string): unknown {
  const url = new URL(`../../../crates/dp-settlement/abi/${contract}.json`, import.meta.url)
  return JSON.parse(readFileSync(fileURLToPath(url), 'utf8'))
}

const CASES = [
  ['DarkPool', darkPoolAbi],
  ['VerifierProxy', verifierProxyAbi],
] as const

describe('contract ABI drift', () => {
  for (const [contract, generated] of CASES) {
    it(`${contract} generated ABI matches the committed JSON (run \`npm run codegen\` if this fails)`, () => {
      const source = loadSourceAbi(contract)
      // Strip the `as const` literal types down to plain JSON for the compare.
      expect(JSON.parse(JSON.stringify(generated))).toEqual(source)
    })
  }
})
```

- [ ] **Step 2: Run it — expect PASS (the generated file is in sync)**

```bash
cd front && npx vitest run lib/contracts/generated.test.ts
```

Expected: 2 passing tests. (This test is green by construction; its value is catching a future hand-edit or un-regenerated JSON. To sanity-check it actually detects drift, temporarily delete one entry from `generated.ts`, re-run → FAIL, then restore with `npm run codegen`.)

- [ ] **Step 3: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/lib/contracts/generated.test.ts
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "test(front): fail CI on contract ABI drift" --date="2026-05-29T12:00:00"
```

---

## Task 3: Pure multicall builder + result mapper

**Files:**
- Create: `front/app/app/trade/_lib/balances/chain-reads.ts`
- Test: `front/app/app/trade/_lib/balances/chain-reads.test.ts`

- [ ] **Step 1: Write the failing test (node — no jsdom)**

```ts
import { describe, it, expect } from 'vitest'

import { buildBalanceContracts, mapBalanceResults } from './chain-reads'

const ADDRS = {
  darkPool: '0x1111111111111111111111111111111111111111',
  weth: '0x2222222222222222222222222222222222222222',
  usdc: '0x3333333333333333333333333333333333333333',
} as const
const TRADER = '0x4444444444444444444444444444444444444444' as const

describe('buildBalanceContracts', () => {
  it('builds 4 calls in order: DarkPool.balances(WETH,USDC) then ERC20.balanceOf(WETH,USDC)', () => {
    const calls = buildBalanceContracts(ADDRS, TRADER)
    expect(calls).toHaveLength(4)
    expect(calls[0]).toMatchObject({ address: ADDRS.darkPool, functionName: 'balances', args: [TRADER, ADDRS.weth] })
    expect(calls[1]).toMatchObject({ address: ADDRS.darkPool, functionName: 'balances', args: [TRADER, ADDRS.usdc] })
    expect(calls[2]).toMatchObject({ address: ADDRS.weth, functionName: 'balanceOf', args: [TRADER] })
    expect(calls[3]).toMatchObject({ address: ADDRS.usdc, functionName: 'balanceOf', args: [TRADER] })
  })
})

describe('mapBalanceResults', () => {
  it('formats raw bigints to decimal strings using per-token decimals (WETH 18dp, USDC 6dp)', () => {
    const out = mapBalanceResults([
      2_000000000000000000n, // internal WETH = 2
      1500_000000n, // internal USDC = 1500
      10_500000000000000000n, // wallet WETH = 10.5
      250_000000n, // wallet USDC = 250
    ])
    expect(out.internal).toEqual({ weth: '2', usdc: '1500' })
    expect(out.wallet).toEqual({ weth: '10.5', usdc: '250' })
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd front && npx vitest run app/app/trade/_lib/balances/chain-reads.test.ts
```

Expected: FAIL — `Failed to resolve import './chain-reads'`.

- [ ] **Step 3: Implement `chain-reads.ts`**

```ts
import { erc20Abi } from 'viem'

import { darkPoolAbi } from '@/lib/contracts/generated'
import type { Address, Balances } from '@/lib/wallet/types'

import { formatRawBalance } from './format-balance'

export interface BalanceAddresses {
  darkPool: Address
  weth: Address
  usdc: Address
}

export const EMPTY_BALANCES: Balances = { weth: '0', usdc: '0' }

/**
 * The four reads, in a fixed order that `mapBalanceResults` relies on:
 *   0: DarkPool.balances(trader, WETH)  → internal WETH
 *   1: DarkPool.balances(trader, USDC)  → internal USDC
 *   2: WETH.balanceOf(trader)           → wallet WETH
 *   3: USDC.balanceOf(trader)           → wallet USDC
 * Returned as a plain array shaped for wagmi's `useReadContracts`.
 */
export function buildBalanceContracts(addrs: BalanceAddresses, trader: Address) {
  return [
    { address: addrs.darkPool, abi: darkPoolAbi, functionName: 'balances', args: [trader, addrs.weth] },
    { address: addrs.darkPool, abi: darkPoolAbi, functionName: 'balances', args: [trader, addrs.usdc] },
    { address: addrs.weth, abi: erc20Abi, functionName: 'balanceOf', args: [trader] },
    { address: addrs.usdc, abi: erc20Abi, functionName: 'balanceOf', args: [trader] },
  ] as const
}

export function mapBalanceResults(results: readonly bigint[]): { wallet: Balances; internal: Balances } {
  const [internalWeth, internalUsdc, walletWeth, walletUsdc] = results
  return {
    internal: { weth: formatRawBalance('WETH', internalWeth), usdc: formatRawBalance('USDC', internalUsdc) },
    wallet: { weth: formatRawBalance('WETH', walletWeth), usdc: formatRawBalance('USDC', walletUsdc) },
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd front && npx vitest run app/app/trade/_lib/balances/chain-reads.test.ts
```

Expected: 2 passing tests.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/app/app/trade/_lib/balances/chain-reads.ts front/app/app/trade/_lib/balances/chain-reads.test.ts
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "feat(front): pure multicall builder + balance mapper for on-chain reads" --date="2026-05-29T12:00:00"
```

---

## Task 4: `useChainBalances` hook (wagmi multicall + event watch)

**Files:**
- Create: `front/app/app/trade/_hooks/balances/useChainBalances.ts`
- Test: `front/app/app/trade/_hooks/balances/useChainBalances.test.ts`

- [ ] **Step 1: Write the failing test (jsdom; fully mock wagmi, config, wallet)**

```ts
// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

const TRADER = '0x4444444444444444444444444444444444444444'

vi.mock('@/lib/config', () => ({
  config: {
    useMocks: false,
    chainId: 31337,
    contracts: {
      darkPool: '0x1111111111111111111111111111111111111111',
      verifierProxy: '0x0000000000000000000000000000000000000000',
      weth: '0x2222222222222222222222222222222222222222',
      usdc: '0x3333333333333333333333333333333333333333',
    },
  },
}))

vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    address: TRADER,
    status: 'connected',
    isConnected: true,
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
}))

const refetch = vi.fn()
let capturedContracts: unknown[] = []
let watchHandlers: Array<() => void> = []

vi.mock('wagmi', () => ({
  useReadContracts: (cfg: { contracts: unknown[]; query?: { enabled?: boolean } }) => {
    capturedContracts = cfg.contracts
    return {
      data: [2_000000000000000000n, 1500_000000n, 10_500000000000000000n, 250_000000n],
      isLoading: false,
      isError: false,
      refetch,
    }
  },
  useWatchContractEvent: (cfg: { enabled?: boolean; onLogs: () => void }) => {
    if (cfg.enabled) watchHandlers.push(cfg.onLogs)
  },
}))

import { useChainBalances } from './useChainBalances'

beforeEach(() => {
  refetch.mockClear()
  capturedContracts = []
  watchHandlers = []
})

describe('useChainBalances', () => {
  it('issues a 4-call multicall and maps results to decimal strings', () => {
    const { result } = renderHook(() => useChainBalances(true))
    expect(capturedContracts).toHaveLength(4)
    expect(result.current.internal).toEqual({ weth: '2', usdc: '1500' })
    expect(result.current.wallet).toEqual({ weth: '10.5', usdc: '250' })
    expect(result.current.isLoading).toBe(false)
    expect(result.current.isError).toBe(false)
  })

  it('registers Deposit + Withdrawal watchers that refetch on log', () => {
    renderHook(() => useChainBalances(true))
    expect(watchHandlers).toHaveLength(2)
    watchHandlers[0]()
    watchHandlers[1]()
    expect(refetch).toHaveBeenCalledTimes(2)
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd front && npx vitest run app/app/trade/_hooks/balances/useChainBalances.test.ts
```

Expected: FAIL — `Failed to resolve import './useChainBalances'`.

- [ ] **Step 3: Implement `useChainBalances.ts`**

```ts
'use client'

import { useReadContracts, useWatchContractEvent } from 'wagmi'

import { config } from '@/lib/config'
import { darkPoolAbi } from '@/lib/contracts/generated'
import { useWallet } from '@/lib/wallet/hooks'
import type { Balances } from '@/lib/wallet/types'

import { buildBalanceContracts, EMPTY_BALANCES, mapBalanceResults } from '../../_lib/balances/chain-reads'

export interface ChainBalances {
  wallet: Balances
  internal: Balances
  isLoading: boolean
  isError: boolean
  refetch: () => void
}

/**
 * Reads on-chain balances for the connected trader. `enabled` is owned
 * by the caller (`useBalances`) so the hook can be called
 * unconditionally (Rules of Hooks) while the underlying query/watchers
 * stay dormant under mocks or with no wallet.
 */
export function useChainBalances(enabled: boolean): ChainBalances {
  const { address } = useWallet()
  const addrs = config.contracts
  const ready = enabled && Boolean(address) && Boolean(addrs)

  const { data, isLoading, isError, refetch } = useReadContracts({
    allowFailure: false,
    contracts: ready ? buildBalanceContracts(addrs!, address!) : [],
    query: { enabled: ready },
  })

  const onLogs = () => {
    void refetch()
  }
  // wagmi narrows `args` from the abi+eventName; `trader` is the indexed
  // first param of both Deposit and Withdrawal.
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Deposit',
    args: address ? { trader: address } : undefined,
    enabled: ready,
    onLogs,
  })
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Withdrawal',
    args: address ? { trader: address } : undefined,
    enabled: ready,
    onLogs,
  })

  const mapped = data ? mapBalanceResults(data as readonly bigint[]) : null
  return {
    wallet: mapped?.wallet ?? EMPTY_BALANCES,
    internal: mapped?.internal ?? EMPTY_BALANCES,
    isLoading,
    isError,
    refetch: onLogs,
  }
}
```

- [ ] **Step 4: Run the test to verify it passes; then typecheck**

```bash
cd front && npx vitest run app/app/trade/_hooks/balances/useChainBalances.test.ts && npm run typecheck
```

Expected: 2 passing tests; `tsc` exits 0. If `tsc` flags the `args: { trader }` filter or the `data` tuple, add a narrow `as` cast at that line only (e.g. `args: { trader: address } as never` is **not** acceptable — instead cast the read result `data as readonly bigint[]`, already present, and if needed type `useWatchContractEvent` args via `{ trader: address }` which wagmi accepts for indexed filters). Re-run typecheck.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/app/app/trade/_hooks/balances/useChainBalances.ts front/app/app/trade/_hooks/balances/useChainBalances.test.ts
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "feat(front): useChainBalances reads on-chain balances + watches deposit/withdrawal" --date="2026-05-29T12:00:00"
```

---

## Task 5: `useBalances` selector (mock ↔ chain)

**Files:**
- Create: `front/app/app/trade/_hooks/balances/useBalances.ts`
- Test: `front/app/app/trade/_hooks/balances/useBalances.test.ts`

- [ ] **Step 1: Write the failing test (jsdom)**

```ts
// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook } from '@testing-library/react'

vi.mock('@/lib/config', () => ({ config: { useMocks: true, contracts: null, chainId: 31337 } }))

let connected = true
vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    isConnected: connected,
    address: connected ? '0x4444444444444444444444444444444444444444' : null,
    status: connected ? 'connected' : 'disconnected',
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
  useWalletBalances: () => ({ weth: '1', usdc: '1000' }),
  useInternalBalances: () => ({ weth: '0', usdc: '0' }),
}))

const chain = {
  wallet: { weth: '2', usdc: '1500' },
  internal: { weth: '0.5', usdc: '50' },
  isLoading: false,
  isError: false,
  refetch: vi.fn(),
}
vi.mock('./useChainBalances', () => ({ useChainBalances: () => chain }))

import { useBalances } from './useBalances'

beforeEach(() => {
  connected = true
})
afterEach(() => {
  vi.unstubAllEnvs()
})

describe('useBalances', () => {
  it('returns mock balances when the global mock switch is on', () => {
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('ready')
    expect(result.current.wallet).toEqual({ weth: '1', usdc: '1000' })
    expect(result.current.internal).toEqual({ weth: '0', usdc: '0' })
  })

  it('returns disconnected status when no wallet is connected', () => {
    connected = false
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('disconnected')
  })

  it('uses chain balances when NEXT_PUBLIC_USE_MOCKS_BALANCES=false', () => {
    vi.stubEnv('NEXT_PUBLIC_USE_MOCKS_BALANCES', 'false')
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('ready')
    expect(result.current.wallet).toEqual({ weth: '2', usdc: '1500' })
    expect(result.current.internal).toEqual({ weth: '0.5', usdc: '50' })
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd front && npx vitest run app/app/trade/_hooks/balances/useBalances.test.ts
```

Expected: FAIL — `Failed to resolve import './useBalances'`.

- [ ] **Step 3: Implement `useBalances.ts`**

```ts
'use client'

import { config } from '@/lib/config'
import { useInternalBalances, useWallet, useWalletBalances } from '@/lib/wallet/hooks'
import type { Balances } from '@/lib/wallet/types'

import { useChainBalances } from './useChainBalances'

export type BalancesStatus = 'disconnected' | 'loading' | 'error' | 'ready'

export interface UseBalancesResult {
  wallet: Balances
  internal: Balances
  status: BalancesStatus
  refetch: () => void
}

const EMPTY: Balances = { weth: '0', usdc: '0' }

/**
 * Per-feature mock switch, falling back to the global one. Read via
 * direct `process.env` property access so Next's NEXT_PUBLIC_* static
 * inlining still works (same constraint as lib/sdk/client.ts).
 */
function balancesUseMocks(): boolean {
  const raw = process.env.NEXT_PUBLIC_USE_MOCKS_BALANCES
  if (raw === 'true' || raw === '1') return true
  if (raw === 'false' || raw === '0') return false
  return config.useMocks
}

export function useBalances(): UseBalancesResult {
  const useMocks = balancesUseMocks()
  const { isConnected } = useWallet()

  // All hooks run unconditionally. Mock-store reads are cheap; the chain
  // query/watchers are disabled unless we're in live mode.
  const mockWallet = useWalletBalances()
  const mockInternal = useInternalBalances()
  const chain = useChainBalances(!useMocks)

  if (!isConnected) {
    return { wallet: EMPTY, internal: EMPTY, status: 'disconnected', refetch: chain.refetch }
  }
  if (useMocks) {
    return { wallet: mockWallet, internal: mockInternal, status: 'ready', refetch: () => {} }
  }
  const status: BalancesStatus = chain.isError ? 'error' : chain.isLoading ? 'loading' : 'ready'
  return { wallet: chain.wallet, internal: chain.internal, status, refetch: chain.refetch }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd front && npx vitest run app/app/trade/_hooks/balances/useBalances.test.ts
```

Expected: 3 passing tests.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/app/app/trade/_hooks/balances/useBalances.ts front/app/app/trade/_hooks/balances/useBalances.test.ts
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "feat(front): useBalances selector switches mock vs on-chain balances" --date="2026-05-29T12:00:00"
```

---

## Task 6: Wire `BalancesPanel` to `useBalances` (loading/error/ready/disconnected)

**Files:**
- Modify: `front/app/app/trade/_components/balances/BalancesPanel.tsx`
- Modify: `front/app/app/trade/_components/balances/BalancesPanel.test.tsx`

`states.tsx` already exports `BalancesLoading`, `BalancesDisconnected`, `BalancesError` — no change needed there.

- [ ] **Step 1: Rewrite the panel test to drive status via a mocked `useBalances`**

This decouples the panel (presentation) from the wallet store and config; the data-source logic is already covered by Task 5. Replace the entire contents of `BalancesPanel.test.tsx` with:

```tsx
// Presentational tests: the panel renders purely from useBalances()'s
// status + balances. Source-selection logic lives in useBalances.test.ts.
import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockResult = {
  wallet: { weth: '0', usdc: '0' },
  internal: { weth: '0', usdc: '0' },
  status: 'disconnected' as 'disconnected' | 'loading' | 'error' | 'ready',
  refetch: vi.fn(),
}
vi.mock('../../_hooks/balances/useBalances', () => ({ useBalances: () => mockResult }))

import { BalancesPanel } from './BalancesPanel'

function render(): string {
  return renderToStaticMarkup(<BalancesPanel />)
}

beforeEach(() => {
  Object.assign(mockResult, {
    wallet: { weth: '0', usdc: '0' },
    internal: { weth: '0', usdc: '0' },
    status: 'disconnected',
  })
})

describe('BalancesPanel', () => {
  it('renders the bracketed-tag header in every state', () => {
    expect(render()).toContain('[ BALANCES ]')
  })

  it('disconnected → connect prompt, no balance columns', () => {
    mockResult.status = 'disconnected'
    const html = render()
    expect(html).toContain('[ CONNECT WALLET ]')
    expect(html).not.toContain('[ WALLET ]')
    expect(html).not.toContain('[ DARKPOOL ]')
  })

  it('loading → skeleton', () => {
    mockResult.status = 'loading'
    expect(render()).toContain('Loading balances')
  })

  it('error → unavailable label', () => {
    mockResult.status = 'error'
    expect(render()).toContain('[ BALANCES UNAVAILABLE ]')
  })

  it('ready → both columns with formatted balances', () => {
    mockResult.status = 'ready'
    mockResult.wallet = { weth: '1', usdc: '1000' }
    mockResult.internal = { weth: '0', usdc: '0' }
    const html = render()
    expect(html).toContain('[ WALLET ]')
    expect(html).toContain('[ DARKPOOL ]')
    expect(html).toContain('WETH')
    expect(html).toContain('USDC')
    expect(html).toContain('1.0000') // WETH wallet, 4dp
    expect(html).toContain('1000.00') // USDC wallet, 2dp
    expect(html).toContain('0.0000') // WETH internal, 4dp
    expect(html).toContain('0.00') // USDC internal, 2dp
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd front && npx vitest run app/app/trade/_components/balances/BalancesPanel.test.tsx
```

Expected: FAIL — the current panel imports the wallet hooks, not `useBalances`, so the `loading`/`error` assertions fail (and the mock module is unused).

- [ ] **Step 3: Rewrite `BalancesPanel.tsx` to consume `useBalances`**

Replace the imports and the `BalancesPanel` function; keep `Header`, `BalancesGrid`, `ColumnHeaderRow`, `TokenRow`, `amountFor` exactly as they are.

Change the import block at the top from:

```tsx
import { useInternalBalances, useWallet, useWalletBalances } from '@/lib/wallet/hooks'
import type { Balances, TokenSymbol } from '@/lib/wallet/types'
import { displayDecimalsFor } from '../../_lib/balances/format-balance'
import { BalancesDisconnected } from './states'
```

to:

```tsx
import type { Balances, TokenSymbol } from '@/lib/wallet/types'
import { useBalances } from '../../_hooks/balances/useBalances'
import { displayDecimalsFor } from '../../_lib/balances/format-balance'
import { BalancesDisconnected, BalancesError, BalancesLoading } from './states'
```

and replace the `BalancesPanel` function body with:

```tsx
export function BalancesPanel() {
  const { wallet, internal, status, refetch } = useBalances()
  const headerId = React.useId()

  return (
    <section
      aria-labelledby={headerId}
      className="flex h-full flex-col border border-brand-border bg-brand-surface"
    >
      <Header id={headerId} />
      {status === 'disconnected' && <BalancesDisconnected />}
      {status === 'loading' && <BalancesLoading />}
      {status === 'error' && <BalancesError onRetry={refetch} />}
      {status === 'ready' && <BalancesGrid wallet={wallet} internal={internal} />}
    </section>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes; then typecheck**

```bash
cd front && npx vitest run app/app/trade/_components/balances/BalancesPanel.test.tsx && npm run typecheck
```

Expected: 5 passing tests; `tsc` exits 0.

- [ ] **Step 5: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/app/app/trade/_components/balances/BalancesPanel.tsx front/app/app/trade/_components/balances/BalancesPanel.test.tsx
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "feat(front): balances panel renders live reads with loading/error states" --date="2026-05-29T12:00:00"
```

---

## Task 7: Document the toggle + full verification

**Files:**
- Modify: `front/.env.local.example`

- [ ] **Step 1: Document `NEXT_PUBLIC_USE_MOCKS_BALANCES`**

Add this block immediately after the `NEXT_PUBLIC_USE_MOCKS=true` line in `front/.env.local.example`:

```bash

# Per-feature override for the balances panel. Unset → follows
# NEXT_PUBLIC_USE_MOCKS. Set to false to read real on-chain balances
# (DarkPool.balances + ERC-20 balanceOf) while other surfaces stay
# mocked. Requires real NEXT_PUBLIC_*_ADDRESS values below.
NEXT_PUBLIC_USE_MOCKS_BALANCES=true
```

- [ ] **Step 2: Full verification sweep**

```bash
cd front && npm run codegen && git diff --exit-code lib/contracts/generated.ts && npm run typecheck && npm run lint && npm test
```

Expected: codegen produces **no** diff (generated file already in sync); `tsc` clean; lint clean; the full Vitest suite passes (existing tests + the ~12 new tests across Tasks 2–6), 0 failures. If `npm run lint` flags the generated file, add `lib/contracts/generated.ts` to `front/.eslintignore` (create it if absent) and re-run.

- [ ] **Step 3: Commit**

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git add front/.env.local.example front/.eslintignore 2>/dev/null
GIT_AUTHOR_DATE="2026-05-29T12:00:00" GIT_COMMITTER_DATE="2026-05-29T12:00:00" \
  git commit -m "docs(front): document NEXT_PUBLIC_USE_MOCKS_BALANCES toggle" --date="2026-05-29T12:00:00"
```

(If `.eslintignore` was not needed, drop it from the `git add`.)

---

## Open the PR

- [ ] Push the branch and open the PR (base `main`, body opens `Closes #91`):

```bash
cd /home/mario/DarkPool-Exchange/.claude/worktrees/feat+issue-91-contract-reads
git push -u origin HEAD
gh pr create --base main --title "[I2.2] Real contract reads (wagmi-generated ABIs + multicall balances)" \
  --body "Closes #91

Generates typed DarkPool/VerifierProxy ABIs via @wagmi/cli, reads real on-chain balances (DarkPool.balances + ERC-20 balanceOf) through a viem multicall, refreshes on Deposit/Withdrawal events, and adds a CI ABI-drift test. Balances panel switches mock↔chain on NEXT_PUBLIC_USE_MOCKS_BALANCES. No Groth16/wallet-provider changes."
```

> Note: per CLAUDE.md, do **not** add a Claude co-author trailer **or** a "Generated with Claude Code" footer to commits or the PR body. Confirm the auto-generated branch name (`worktree-feat+issue-91-contract-reads`) is acceptable, or rename to `feat/issue-91-contract-reads` before pushing.

---

## Self-review (author check — completed)

- **Spec coverage:** wagmi.config.ts (T1) · `npm run codegen` wiring (T1) · panel calls `balances()` via multicall + `useWatchContractEvent` (T3/T4/T6) · ABI-drift CI test (T2) · mock toggle (T5/T7) · loading/error states (T6). All spec sections map to a task.
- **Placeholder scan:** every code step has complete code; no TBDs.
- **Type consistency:** `Balances {weth,usdc}`, `BalanceAddresses {darkPool,weth,usdc}`, `ChainBalances`, `UseBalancesResult`, `BalancesStatus`, and the `buildBalanceContracts`/`mapBalanceResults` names are used identically across Tasks 3–6. The 4-call order in `buildBalanceContracts` matches the destructure in `mapBalanceResults` (internal WETH, internal USDC, wallet WETH, wallet USDC).
