'use client'

// Drives the multi-stage mock submission (PREPARING WITNESS → GENERATING
// PROOF → ENCRYPTING → SUBMITTING → success/error). The same state shape
// will be reused by F1.12 (#79) onboarding so the stage IDs are stable.
//
// The orchestration runs inside `runSubmission` — a pure async function
// that emits each phase transition to an `onPhase` callback. The hook
// itself is a thin wrapper that pipes those emissions into React state.
// That split lets unit tests drive the state machine without a DOM and
// without React Testing Library (the project's vitest setup is
// node-only).
//
// Two dependencies are injected so tests can run synchronously and the
// real submission path can be swapped in Phase 2:
//   - `placeOrder`: mock-store mutation today, real RPC after #99.
//   - `delay`:      defaults to setTimeout-based sleep; tests pass a
//                   manually-pumped scheduler.

import { useCallback, useEffect, useRef, useState } from 'react'

import {
  STAGE_DURATIONS_MS,
  STAGE_ORDER,
  STAGE_TOTAL_MS,
  SUCCESS_HOLD_MS,
  type SubmitStageId,
} from './policy'

export type SubmissionPhase =
  | { kind: 'idle' }
  | { kind: 'running'; stage: SubmitStageId; progress: number }
  | { kind: 'success' }
  | { kind: 'error'; message: string }

export interface SubmitPayload {
  side: 'buy' | 'sell'
  price: string
  size: string
}

export interface SubmissionDeps {
  placeOrder: (payload: SubmitPayload) => void | Promise<void>
  /** Returns a promise that resolves after `ms`. Defaults to setTimeout. */
  delay?: (ms: number) => Promise<void>
}

export interface RunSubmissionOptions extends SubmissionDeps {
  onPhase: (phase: SubmissionPhase) => void
  /**
   * `true` aborts the run between awaits. Used by the hook to drop
   * stale runs when `submit` fires while another run is still in flight.
   */
  shouldAbort?: () => boolean
}

export function progressAtStartOfStage(stage: SubmitStageId): number {
  let elapsed = 0
  for (const id of STAGE_ORDER) {
    if (id === stage) break
    elapsed += STAGE_DURATIONS_MS[id]
  }
  return elapsed / STAGE_TOTAL_MS
}

export function progressAtEndOfStage(stage: SubmitStageId): number {
  let elapsed = 0
  for (const id of STAGE_ORDER) {
    elapsed += STAGE_DURATIONS_MS[id]
    if (id === stage) break
  }
  return elapsed / STAGE_TOTAL_MS
}

const defaultDelay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

/**
 * Pure orchestrator. Emits each phase change through `onPhase` and
 * resolves once the run lands on a terminal state (success / error) and
 * the post-success hold has elapsed.
 */
export async function runSubmission(
  payload: SubmitPayload,
  opts: RunSubmissionOptions
): Promise<void> {
  const delay = opts.delay ?? defaultDelay
  const aborted = () => (opts.shouldAbort ? opts.shouldAbort() : false)

  try {
    for (const stage of STAGE_ORDER) {
      if (aborted()) return
      opts.onPhase({ kind: 'running', stage, progress: progressAtStartOfStage(stage) })

      // placeOrder runs at the start of the SUBMIT stage so the
      // mock-store mutation lands while the user still sees the
      // "SUBMITTING" label. A synchronous throw surfaces here.
      if (stage === 'submitting') {
        await Promise.resolve(opts.placeOrder(payload))
      }

      await delay(STAGE_DURATIONS_MS[stage])
      if (aborted()) return
      opts.onPhase({ kind: 'running', stage, progress: progressAtEndOfStage(stage) })
    }

    if (aborted()) return
    opts.onPhase({ kind: 'success' })

    await delay(SUCCESS_HOLD_MS)
    if (aborted()) return
    opts.onPhase({ kind: 'idle' })
  } catch (err) {
    if (aborted()) return
    const message = err instanceof Error ? err.message : String(err)
    opts.onPhase({ kind: 'error', message })
  }
}

export interface UseSubmitStagesParams extends SubmissionDeps {
  onSuccess?: (payload: SubmitPayload) => void
  onError?: (error: Error) => void
}

export interface UseSubmitStagesResult {
  phase: SubmissionPhase
  isRunning: boolean
  submit: (payload: SubmitPayload) => Promise<void>
  reset: () => void
}

export function useSubmitStages(params: UseSubmitStagesParams): UseSubmitStagesResult {
  const [phase, setPhase] = useState<SubmissionPhase>({ kind: 'idle' })
  const runIdRef = useRef(0)
  const paramsRef = useRef(params)
  useEffect(() => {
    paramsRef.current = params
  })

  const reset = useCallback(() => {
    runIdRef.current += 1
    setPhase({ kind: 'idle' })
  }, [])

  const submit = useCallback(async (payload: SubmitPayload) => {
    const myRunId = ++runIdRef.current
    const isStale = () => runIdRef.current !== myRunId

    await runSubmission(payload, {
      placeOrder: paramsRef.current.placeOrder,
      delay: paramsRef.current.delay,
      shouldAbort: isStale,
      onPhase: (next) => {
        if (isStale()) return
        setPhase(next)
        if (next.kind === 'success') paramsRef.current.onSuccess?.(payload)
        if (next.kind === 'error') paramsRef.current.onError?.(new Error(next.message))
      },
    })
  }, [])

  return {
    phase,
    isRunning: phase.kind === 'running',
    submit,
    reset,
  }
}
