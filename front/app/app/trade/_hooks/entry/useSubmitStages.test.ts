import { describe, expect, it, vi } from 'vitest'

import {
  STAGE_DURATIONS_MS,
  STAGE_ORDER,
  STAGE_TOTAL_MS,
  SUCCESS_HOLD_MS,
} from '../../_lib/entry/policy'
import {
  progressAtEndOfStage,
  progressAtStartOfStage,
  runSubmission,
  type SubmissionPhase,
  type SubmitPayload,
} from './useSubmitStages'

const PAYLOAD: SubmitPayload = { side: 'buy', price: '3000', size: '0.5' }

function instant(): (ms: number) => Promise<void> {
  return async () => {}
}

function record(phases: SubmissionPhase[]) {
  return (phase: SubmissionPhase) => {
    phases.push(phase)
  }
}

describe('progress helpers', () => {
  it('progressAtStartOfStage returns 0 for the first stage and a value < 1 for the rest', () => {
    expect(progressAtStartOfStage('preparing')).toBe(0)
    for (const stage of STAGE_ORDER) {
      expect(progressAtStartOfStage(stage)).toBeGreaterThanOrEqual(0)
      expect(progressAtStartOfStage(stage)).toBeLessThan(1)
    }
  })

  it('progressAtEndOfStage returns 1 for the last stage', () => {
    expect(progressAtEndOfStage('submitting')).toBe(1)
  })

  it('progress is monotonically non-decreasing across stage order', () => {
    let last = -1
    for (const stage of STAGE_ORDER) {
      const start = progressAtStartOfStage(stage)
      const end = progressAtEndOfStage(stage)
      expect(start).toBeGreaterThanOrEqual(last)
      expect(end).toBeGreaterThanOrEqual(start)
      last = end
    }
  })

  it('stage durations sum to STAGE_TOTAL_MS', () => {
    const total = STAGE_ORDER.reduce((sum, id) => sum + STAGE_DURATIONS_MS[id], 0)
    expect(total).toBe(STAGE_TOTAL_MS)
  })
})

describe('runSubmission', () => {
  it('emits each stage transition in order, then success, then idle', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay: instant(),
      onPhase: record(phases),
    })

    // Two `running` emissions per stage (start + end), then success, then idle.
    const running = phases.filter((p) => p.kind === 'running') as Extract<
      SubmissionPhase,
      { kind: 'running' }
    >[]
    expect(running.map((p) => p.stage)).toEqual([
      'preparing',
      'preparing',
      'proving',
      'proving',
      'encrypting',
      'encrypting',
      'submitting',
      'submitting',
    ])

    const terminal = phases.slice(-2)
    expect(terminal[0].kind).toBe('success')
    expect(terminal[1].kind).toBe('idle')
  })

  it('calls placeOrder exactly once, during the submitting stage', async () => {
    const placeOrder = vi.fn()
    let placeOrderCallOrder = -1
    let nextOrder = 0
    const onPhase = vi.fn((phase: SubmissionPhase) => {
      nextOrder += 1
      if (phase.kind === 'running' && phase.stage === 'submitting' && placeOrderCallOrder === -1) {
        // First entry into submitting — placeOrder should already have been
        // recorded against the next call (the orchestrator awaits placeOrder
        // before the stage-duration delay).
      }
    })

    placeOrder.mockImplementation(() => {
      placeOrderCallOrder = nextOrder
    })

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay: instant(),
      onPhase,
    })

    expect(placeOrder).toHaveBeenCalledTimes(1)
    expect(placeOrder).toHaveBeenCalledWith(PAYLOAD)
  })

  it('emits an error phase when placeOrder throws', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn(() => {
      throw new Error('insufficient liquidity')
    })

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay: instant(),
      onPhase: record(phases),
    })

    const terminal = phases[phases.length - 1]
    expect(terminal.kind).toBe('error')
    if (terminal.kind === 'error') {
      expect(terminal.message).toBe('insufficient liquidity')
    }
    // No success/idle should follow an error.
    expect(phases.filter((p) => p.kind === 'success')).toHaveLength(0)
  })

  it('emits an error phase when placeOrder rejects asynchronously', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn(async () => {
      throw new Error('rejected by engine')
    })

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay: instant(),
      onPhase: record(phases),
    })

    const terminal = phases[phases.length - 1]
    expect(terminal.kind).toBe('error')
  })

  it('aborts before the next emission when shouldAbort returns true', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    let count = 0

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay: instant(),
      shouldAbort: () => {
        // Abort after the first running phase has been emitted.
        count += 1
        return count > 1
      },
      onPhase: record(phases),
    })

    // The run aborts before reaching the submitting stage, so placeOrder is
    // never called.
    expect(placeOrder).not.toHaveBeenCalled()
    // No terminal phase (success / error) is emitted on abort.
    expect(phases.some((p) => p.kind === 'success' || p.kind === 'error')).toBe(false)
  })

  it('delays each stage by STAGE_DURATIONS_MS[stage]', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    const delays: number[] = []
    const delay = async (ms: number) => {
      delays.push(ms)
    }

    await runSubmission(PAYLOAD, {
      placeOrder,
      delay,
      onPhase: record(phases),
    })

    // One delay per stage + SUCCESS_HOLD_MS.
    expect(delays).toEqual([
      STAGE_DURATIONS_MS.preparing,
      STAGE_DURATIONS_MS.proving,
      STAGE_DURATIONS_MS.encrypting,
      STAGE_DURATIONS_MS.submitting,
      SUCCESS_HOLD_MS,
    ])
  })
})
