# [I2.3] On-chain deposit & withdraw flows — design

**Issue:** #92 · **Epic:** #62 · **Depends on:** #91 (I2.2, merged)

## Goal

Replace the F1.5 mock deposit/withdraw modals with real ERC20 `approve` +
`DarkPool.deposit` / `DarkPool.withdraw` calls in **live mode**, while keeping
the mock path fully intact for `NEXT_PUBLIC_USE_MOCKS=true`.

### Acceptance criteria (from the issue)

1. Deposit: show current allowance; prompt `approve` only when needed, for the
   **exact amount** (never `type(uint256).max`), then `deposit`.
2. Withdraw: validate `amount <= balances[trader][token]` from the latest read;
   disabled when `paused()` is true (banner explains).
3. Transaction lifecycle (`isPending` / `isConfirming` / `isConfirmed`) reflected
   in modal copy.
4. Specific error mapping: user reject, gas-estimation revert, allowance race,
   contract paused, zero amount.
5. On a confirmed event matching the trader, the balances panel refreshes
   automatically.

## Strategy: mirror `useBalances`, never break F1.5

A per-feature flag `depositUseMocks()` = `NEXT_PUBLIC_USE_MOCKS_DEPOSIT` ??
`config.useMocks` (same pattern as `balancesUseMocks`). Facade hooks call **both**
the mock and live hooks unconditionally (Rules of Hooks) and return the active
one — exactly the `useBalances` pattern. Mock mode keeps the existing
`setTimeout` + `walletStore` path; live mode swaps in wagmi.

The app always mounts a `WagmiProvider` (even in mock mode), so wagmi hooks are
always safe at runtime. Node static-markup tests have no provider, so any test
that renders a component invoking wagmi mocks `wagmi` (and `@/lib/config`) — the
established pattern from `useChainBalances.test.ts` / `BalancesPanel.test.tsx`.

## Stage machine (extended)

`_lib/deposit/stage-machine.ts`:

- `Stage` gains `phase?: 'signing' | 'mining'`, meaningful only when `kind` is
  `approving` | `submitting`.
- New action `signed` (tx hash received, now awaiting receipt).
- Transitions:
  - `start{needsApproval}` → `{ kind: needsApproval?'approving':'submitting', phase:'signing' }`
  - `signed` → keep `kind`, set `phase:'mining'` (only from a `phase:'signing'` in-flight stage)
  - `approvalDone` → `{ kind:'submitting', phase:'signing' }` (only from `approving`)
  - `submitted` → `{ kind:'confirmed' }` (only from `submitting`)
  - `fail` → `{ kind:'error', errorMessage }` (only from in-flight)
  - `reset` → `idle`

Copy mapping (both modes):
- `phase:'signing'` → **"CONFIRM IN WALLET…"** (maps to wagmi `isPending`)
- `phase:'mining'` → **"APPROVING…" / "DEPOSITING…" / "WITHDRAWING…"** (`isConfirming`)
- `confirmed` → **"CONFIRMED"** (`isConfirmed`)

The mock controllers dispatch `signed` so both paths share copy.
`stage-machine.test.ts` updated for the new field + action.

## New modules (all in deposit/balances scope)

### `_lib/deposit/errors.ts` (pure, unit-tested)

`mapTxError(err): { reason: TxErrorReason; message: string }` mapping
viem/wagmi error shapes → AC#4 categories:

| reason            | trigger                                                        | copy (brutalist, no color) |
|-------------------|----------------------------------------------------------------|----------------------------|
| `user-rejected`   | `UserRejectedRequestError` (code 4001)                         | "Signature rejected in wallet." |
| `paused`          | revert `EnforcedPause()` / reason contains "paused"            | "Contract is paused." |
| `zero-amount`     | revert reason `"zero amount"`                                  | "Amount must be greater than zero." |
| `allowance-race`  | revert reason matches ERC20 insufficient-allowance             | "Allowance changed — re-approve and retry." |
| `insufficient`    | revert reason `"insufficient balance"` / ERC20 balance         | "Insufficient balance." |
| `reverted`        | other `ContractFunctionExecutionError` / gas-estimation revert | "Transaction reverted." |
| `unknown`         | fallback                                                       | "Transaction failed." |

Detection walks the viem error `.walk()` / `.shortMessage` / `.cause`; tested
against synthetic error objects (no chain needed).

### `_hooks/deposit/useDepositChainState.ts`

Live `{ allowances: Balances, paused: boolean, refetch }`:
- `useReadContracts` (allowFailure:false) → `ERC20.allowance(trader, darkPool)`
  for WETH + USDC, and `DarkPool.paused()`.
- `useWatchContractEvent` on `Paused` / `Unpaused` and ERC20 `Approval`
  (owner=trader, spender=darkPool) → refetch.
- `enabled` owned by caller; dormant under mocks / no wallet.
- Tested with `vi.mock('wagmi')` (jsdom) — assert contract shape + refetch wiring.

### `_hooks/deposit/live-controllers.ts`

Implement the `DepositController` / `WithdrawController` interfaces so the form
is agnostic. Use `useWriteContract` (async) + `waitForTransactionReceipt`
(checking `receipt.status` — a mined-but-reverted receipt routes to `fail`):

- Deposit: read fresh allowance for the token; if `< amount`, `approve(darkPool,
  amount)` (exact), `signed`→`approvalDone` on receipt, then
  `deposit(token, amount)`, `signed`→`submitted` on receipt. Never
  `maxUint256`.
- Withdraw: `withdraw(token, amount)` directly.
- On confirm: refetch `useDepositChainState` + balances (the `Deposit`/
  `Withdrawal` watchers in `useChainBalances` also fire).
- Errors routed through `mapTxError` → `fail`.
- `simulateRevert` is a no-op in live mode (dev affordance is mock-only; the
  form hides it when `!IS_DEV` and the live path ignores it).
- Tested with `vi.mock('wagmi')` (jsdom, `renderHook`): assert the write args
  (exact-amount approve, correct fn/args), stage progression, error mapping.

## Facade hooks (`_hooks/deposit/hooks.ts`)

- Rename existing → `useMockDepositController` / `useMockWithdrawController`.
- Add `useDepositController()` / `useWithdrawController()` facades: call mock +
  live (with `enabled = !depositUseMocks()`) and return the active one.
- Add `useDepositTxState()` → unified `{ paused, allowances }` from mock
  `useTxState` vs `useDepositChainState`.
- `useTxState` (mock) stays exported for existing callers/tests.

## Forms (minimal change)

`DepositForm` / `WithdrawForm`:
- Wallet balance ← `useBalances().wallet`; DarkPool balance ← `useBalances().internal`.
- `useTxState` → `useDepositTxState()`.
- Phase-aware `primaryLabel` + StepIndicator status line ("CONFIRM IN WALLET…" /
  "…ING…" / "CONFIRMED").
- Validation, layout, accent budget, paused/error notices unchanged.

`DepositForm.test.tsx` / `WithdrawModal.test.tsx`: add
`vi.mock('@/lib/config', () => ({ config: { useMocks: true, … } }))` + inert
`vi.mock('wagmi', …)` so node SSR stays in mock mode. Existing assertions
preserved (additive).

## AC#5 — auto-refresh

Already wired in `useChainBalances`: `useWatchContractEvent` on `Deposit` /
`Withdrawal` filtered to `trader` → `refetch`. Live controller additionally
refetches chain-state on confirm. No change needed to the balances panel.

## Out of scope / non-goals

- No `maxUint256` infinite approval.
- No multi-pair (single ETH/USDC).
- No EIP-712 / Permit2 (contract has none).
- Landing surface and sibling panels untouched.

## Testing summary (TDD)

| Unit | Env | Mocks |
|------|-----|-------|
| `stage-machine.test.ts` | node | none |
| `errors.test.ts` | node | synthetic viem errors |
| `useDepositChainState.test.ts` | jsdom | wagmi, config, wallet/hooks |
| `live-controllers.test.ts` (deposit + withdraw) | jsdom | wagmi, config, wallet/hooks |
| `DepositForm.test.tsx` / `WithdrawModal.test.tsx` | node | config(useMocks:true), wagmi(inert) |

Full suite: `npm run test` (vitest run). Type-check + lint per repo scripts.
