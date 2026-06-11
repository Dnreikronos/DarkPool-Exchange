'use client'

import * as React from 'react'

import { config } from '@/lib/config'
import { walletStore, type TxState } from '@/lib/wallet/mock-store'
import type { Balances, TokenSymbol } from '@/lib/wallet/types'

import { reduceStage, INITIAL_STAGE, type Stage } from '../../_lib/deposit/stage-machine'
import { needsApproval } from '../../_lib/deposit/validation'
import { useDepositChainState } from './useDepositChainState'
import { useLiveDepositController, useLiveWithdrawController } from './live-controllers'

// Step cadence per the F1.5 spec: each fake on-chain step takes ~1s.
// Exposed so tests / stories can override without monkey-patching
// `setTimeout`. Kept on a single object so a future caller can swap
// the whole timing profile (e.g. for `prefers-reduced-motion`) at once.
export interface StepTiming {
  approveMs: number
  submitMs: number
}

export const DEFAULT_STEP_TIMING: StepTiming = {
  approveMs: 1000,
  submitMs: 1000,
}

/**
 * Per-feature mock switch, falling back to the global one — mirrors
 * `balancesUseMocks`. Read via direct `process.env` property access so
 * Next's NEXT_PUBLIC_* static inlining still works.
 */
export function depositUseMocks(): boolean {
  const raw = process.env.NEXT_PUBLIC_USE_MOCKS_DEPOSIT
  if (raw === 'true' || raw === '1') return true
  if (raw === 'false' || raw === '0') return false
  return config.useMocks
}

/** Read the mock tx-state slice (paused + allowances) reactively. */
export function useTxState(): TxState {
  return React.useSyncExternalStore(
    walletStore.subscribe,
    walletStore.getTxState,
    walletStore.getTxState
  )
}

export interface DepositTxState {
  paused: boolean
  allowances: Balances
}

/**
 * Unified `{ paused, allowances }` for the deposit form: mock store under
 * mocks, on-chain reads in live mode. Calls both sources unconditionally
 * (Rules of Hooks) and returns the active one — same shape as `useBalances`.
 */
export function useDepositTxState(): DepositTxState {
  const useMocks = depositUseMocks()
  const mock = useTxState()
  const chain = useDepositChainState(!useMocks)
  if (useMocks) return { paused: mock.paused, allowances: mock.allowances }
  return { paused: chain.paused, allowances: chain.allowances }
}

export type DepositRevertReason = 'approve' | 'deposit'
export type WithdrawRevertReason = 'withdraw'

interface UseTxControllerOptions {
  timing?: StepTiming
}

export interface DepositController {
  stage: Stage
  isPaused: boolean
  /** Trigger the deposit flow; safe to call only when the stage is idle. */
  start(args: { token: TokenSymbol; amount: string }): void
  /**
   * Prime the next start() to revert. The reason determines which step
   * fails — used by the dev-only "Simulate revert" affordance. No-op in
   * live mode.
   */
  simulateRevert(reason: DepositRevertReason): void
  reset(): void
}

export function useMockDepositController({
  timing = DEFAULT_STEP_TIMING,
}: UseTxControllerOptions = {}): DepositController {
  const [stage, dispatch] = React.useReducer(reduceStage, INITIAL_STAGE)
  const tx = useTxState()
  // Refs survive across re-renders without re-triggering effects. The
  // revert flag is consumed by start() and cleared once read.
  const revertRef = React.useRef<DepositRevertReason | null>(null)
  const cancelRef = React.useRef<() => void>(() => {})

  const simulateRevert = React.useCallback((reason: DepositRevertReason) => {
    revertRef.current = reason
  }, [])

  const reset = React.useCallback(() => {
    cancelRef.current()
    revertRef.current = null
    dispatch({ type: 'reset' })
  }, [])

  const start = React.useCallback(
    ({ token, amount }: { token: TokenSymbol; amount: string }) => {
      cancelRef.current()
      const tokenKey = token === 'WETH' ? 'weth' : 'usdc'
      const currentAllowance = walletStore.getTxState().allowances[tokenKey]
      const requiresApproval = needsApproval(amount, currentAllowance)

      dispatch({ type: 'start', needsApproval: requiresApproval })
      // No wallet prompt to wait on in the mock — go straight to "mining".
      dispatch({ type: 'signed' })

      let cancelled = false
      cancelRef.current = () => {
        cancelled = true
      }

      const reject = (msg: string) => {
        if (cancelled) return
        dispatch({ type: 'fail', message: msg })
      }

      const runSubmit = () => {
        const submitTimer = setTimeout(() => {
          if (cancelled) return
          if (revertRef.current === 'deposit') {
            revertRef.current = null
            reject('Deposit reverted.')
            return
          }
          try {
            walletStore.deposit(token, amount)
          } catch (err) {
            const msg = err instanceof Error ? err.message : 'Deposit reverted.'
            reject(msg)
            return
          }
          if (cancelled) return
          dispatch({ type: 'submitted' })
        }, timing.submitMs)
        cancelRef.current = () => {
          cancelled = true
          clearTimeout(submitTimer)
        }
      }

      if (requiresApproval) {
        const approveTimer = setTimeout(() => {
          if (cancelled) return
          if (revertRef.current === 'approve') {
            revertRef.current = null
            reject('Approve reverted.')
            return
          }
          try {
            walletStore.approve(token, amount)
          } catch (err) {
            const msg = err instanceof Error ? err.message : 'Approve reverted.'
            reject(msg)
            return
          }
          if (cancelled) return
          dispatch({ type: 'approvalDone' })
          dispatch({ type: 'signed' })
          runSubmit()
        }, timing.approveMs)
        cancelRef.current = () => {
          cancelled = true
          clearTimeout(approveTimer)
        }
      } else {
        runSubmit()
      }
    },
    [timing.approveMs, timing.submitMs]
  )

  React.useEffect(() => {
    return () => cancelRef.current()
  }, [])

  return {
    stage,
    isPaused: tx.paused,
    start,
    simulateRevert,
    reset,
  }
}

export interface WithdrawController {
  stage: Stage
  isPaused: boolean
  start(args: { token: TokenSymbol; amount: string }): void
  simulateRevert(reason: WithdrawRevertReason): void
  reset(): void
}

export function useMockWithdrawController({
  timing = DEFAULT_STEP_TIMING,
}: UseTxControllerOptions = {}): WithdrawController {
  const [stage, dispatch] = React.useReducer(reduceStage, INITIAL_STAGE)
  const tx = useTxState()
  const revertRef = React.useRef<WithdrawRevertReason | null>(null)
  const cancelRef = React.useRef<() => void>(() => {})

  const simulateRevert = React.useCallback((reason: WithdrawRevertReason) => {
    revertRef.current = reason
  }, [])

  const reset = React.useCallback(() => {
    cancelRef.current()
    revertRef.current = null
    dispatch({ type: 'reset' })
  }, [])

  const start = React.useCallback(
    ({ token, amount }: { token: TokenSymbol; amount: string }) => {
      cancelRef.current()
      dispatch({ type: 'start', needsApproval: false })
      dispatch({ type: 'signed' })

      let cancelled = false
      const timer = setTimeout(() => {
        if (cancelled) return
        if (revertRef.current === 'withdraw') {
          revertRef.current = null
          dispatch({ type: 'fail', message: 'Withdraw reverted.' })
          return
        }
        try {
          walletStore.withdraw(token, amount)
        } catch (err) {
          const msg = err instanceof Error ? err.message : 'Withdraw reverted.'
          dispatch({ type: 'fail', message: msg })
          return
        }
        dispatch({ type: 'submitted' })
      }, timing.submitMs)

      cancelRef.current = () => {
        cancelled = true
        clearTimeout(timer)
      }
    },
    [timing.submitMs]
  )

  React.useEffect(() => {
    return () => cancelRef.current()
  }, [])

  return {
    stage,
    isPaused: tx.paused,
    start,
    simulateRevert,
    reset,
  }
}

/**
 * Source-selecting facades: call both the mock and live controllers
 * unconditionally (Rules of Hooks) and return whichever the per-feature
 * mock switch selects. The form depends only on these.
 */
export function useDepositController(options: UseTxControllerOptions = {}): DepositController {
  const useMocks = depositUseMocks()
  const mock = useMockDepositController(options)
  const live = useLiveDepositController(!useMocks)
  return useMocks ? mock : live
}

export function useWithdrawController(options: UseTxControllerOptions = {}): WithdrawController {
  const useMocks = depositUseMocks()
  const mock = useMockWithdrawController(options)
  const live = useLiveWithdrawController(!useMocks)
  return useMocks ? mock : live
}
