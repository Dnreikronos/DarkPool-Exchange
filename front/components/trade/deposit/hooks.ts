'use client'

import * as React from 'react'

import { walletStore, type TxState } from '../../../lib/wallet/mock-store'
import type { TokenSymbol } from '../../../lib/wallet/types'

import { reduceStage, INITIAL_STAGE, type Stage } from './stage-machine'
import { needsApproval } from './validation'

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

/** Read the tx-state slice (paused + allowances) reactively. */
export function useTxState(): TxState {
  return React.useSyncExternalStore(
    walletStore.subscribe,
    walletStore.getTxState,
    walletStore.getTxState
  )
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
   * fails — used by the dev-only "Simulate revert" affordance.
   */
  simulateRevert(reason: DepositRevertReason): void
  reset(): void
}

export function useDepositController({
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

export function useWithdrawController({
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
