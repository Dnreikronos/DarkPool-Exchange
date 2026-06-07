'use client'

// Drives the multi-stage submission (PREPARING WITNESS → GENERATING PROOF →
// ENCRYPTING → SUBMITTING → success/error). The orchestrator walks an
// injected ordered list of StageStep — the mock path (buildMockSteps) and
// the real path (createRealSteps, see _lib/entry/build-submission.ts) both
// assemble that list, so one node-testable state machine drives both.
//
// `stageStartedAtMs` is stamped on each running emission so the view can
// show real elapsed seconds per stage. Errors are routed through an
// injectable `mapError` (default mapSubmissionError) so a DarkPoolError
// becomes specific copy + structured detail for the inline error area.

import { useCallback, useLayoutEffect, useRef, useState } from 'react'

import {
  STAGE_DURATIONS_MS,
  STAGE_ORDER,
  STAGE_TOTAL_MS,
  SUCCESS_HOLD_MS,
  type SubmitStageId,
} from '../../_lib/entry/policy'
import { randomHex, type StageStep } from '../../_lib/entry/build-submission'
import { mapSubmissionError, type SubmitErrorDetail } from '../../_lib/entry/submit-error'

export type { StageStep } from '../../_lib/entry/build-submission'

export type SubmissionPhase =
  | { kind: 'idle' }
  | { kind: 'running'; stage: SubmitStageId; progress: number; stageStartedAtMs?: number }
  | { kind: 'success' }
  | { kind: 'error'; message: string; detail?: SubmitErrorDetail }

export interface SubmitPayload {
  side: 'buy' | 'sell'
  price: string
  size: string
}

export interface RunSubmissionOptions {
  onPhase: (phase: SubmissionPhase) => void
  /** Resolves after `ms`. Defaults to setTimeout. */
  delay?: (ms: number) => Promise<void>
  /** Monotonic clock for the elapsed ticker. Defaults to Date.now. */
  now?: () => number
  /** `true` aborts the run between awaits (drops stale runs). */
  shouldAbort?: () => boolean
  /** Maps a thrown value to an error phase. Defaults to mapSubmissionError. */
  mapError?: (err: unknown) => { message: string; detail?: SubmitErrorDetail }
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
const defaultNow = () => Date.now()

/**
 * Mock steps: a fixed delay per stage (matching STAGE_DURATIONS_MS) with the
 * mock placeOrder fired during `submitting`. Reproduces the F1.9 behaviour
 * for the demo/Storybook path. `prove`, when supplied, replaces the proving
 * delay (legacy parity).
 */
export function buildMockSteps(
  payload: SubmitPayload,
  opts: {
    placeOrder: (payload: SubmitPayload) => void | Promise<void>
    prove?: (witness: {
      commitment_key: string
      side: number
      price: string
      size: string
      salt_hex: string
    }) => Promise<unknown>
    delay?: (ms: number) => Promise<void>
  }
): StageStep[] {
  const delay = opts.delay ?? defaultDelay
  return STAGE_ORDER.map((id) => ({
    id,
    run: async () => {
      if (id === 'proving' && opts.prove) {
        await opts.prove({
          commitment_key: randomHex(32),
          side: payload.side === 'buy' ? 0 : 1,
          price: payload.price,
          size: payload.size,
          salt_hex: randomHex(32),
        })
        return
      }
      if (id === 'submitting') {
        await Promise.resolve(opts.placeOrder(payload))
      }
      await delay(STAGE_DURATIONS_MS[id])
    },
  }))
}

/**
 * Pure orchestrator. Emits each phase change through `onPhase` and resolves
 * once the run lands on a terminal state and the post-success hold elapses.
 */
export async function runSubmission(steps: StageStep[], opts: RunSubmissionOptions): Promise<void> {
  const delay = opts.delay ?? defaultDelay
  const now = opts.now ?? defaultNow
  const mapError = opts.mapError ?? mapSubmissionError
  const aborted = () => (opts.shouldAbort ? opts.shouldAbort() : false)

  try {
    for (const step of steps) {
      if (aborted()) return
      const stageStartedAtMs = now()
      opts.onPhase({
        kind: 'running',
        stage: step.id,
        progress: progressAtStartOfStage(step.id),
        stageStartedAtMs,
      })

      await step.run({ aborted })

      if (aborted()) return
      opts.onPhase({
        kind: 'running',
        stage: step.id,
        progress: progressAtEndOfStage(step.id),
        stageStartedAtMs,
      })
    }

    if (aborted()) return
    opts.onPhase({ kind: 'success' })

    await delay(SUCCESS_HOLD_MS)
    if (aborted()) return
    opts.onPhase({ kind: 'idle' })
  } catch (err) {
    if (aborted()) return
    const { message, detail } = mapError(err)
    opts.onPhase({ kind: 'error', message, detail })
  }
}

export interface UseSubmitStagesParams {
  /** Build the ordered steps for a given form payload (mock or real). */
  buildSteps: (payload: SubmitPayload) => StageStep[]
  onSuccess?: (payload: SubmitPayload) => void
  onError?: (error: Error) => void
  delay?: (ms: number) => Promise<void>
  now?: () => number
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
  // useLayoutEffect over useEffect so paramsRef is current before any
  // post-commit click handler can call submit() — useEffect would leave
  // a one-render window where stale params are visible.
  useLayoutEffect(() => {
    paramsRef.current = params
  })

  const reset = useCallback(() => {
    runIdRef.current += 1
    setPhase({ kind: 'idle' })
  }, [])

  const submit = useCallback(async (payload: SubmitPayload) => {
    const myRunId = ++runIdRef.current
    const isStale = () => runIdRef.current !== myRunId
    const p = paramsRef.current

    await runSubmission(p.buildSteps(payload), {
      delay: p.delay,
      now: p.now,
      shouldAbort: isStale,
      onPhase: (next) => {
        if (isStale()) return
        setPhase(next)
        if (next.kind === 'success') p.onSuccess?.(payload)
        if (next.kind === 'error') p.onError?.(new Error(next.message))
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
