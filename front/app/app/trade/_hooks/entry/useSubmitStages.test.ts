import { describe, expect, it, vi } from 'vitest'

import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/api-client'

import { STAGE_DURATIONS_MS, STAGE_ORDER, SUCCESS_HOLD_MS } from '../../_lib/entry/policy'
import {
  buildMockSteps,
  progressAtEndOfStage,
  progressAtStartOfStage,
  runSubmission,
  type SubmissionPhase,
  type SubmitPayload,
} from './useSubmitStages'

const PAYLOAD: SubmitPayload = { side: 'buy', price: '3000', size: '0.5' }

const instant = () => async () => {}

function record(phases: SubmissionPhase[]) {
  return (phase: SubmissionPhase) => {
    phases.push(phase)
  }
}

describe('progress helpers', () => {
  it('progressAtStartOfStage is 0 for the first stage and < 1 for all', () => {
    expect(progressAtStartOfStage('preparing')).toBe(0)
    for (const stage of STAGE_ORDER) {
      expect(progressAtStartOfStage(stage)).toBeGreaterThanOrEqual(0)
      expect(progressAtStartOfStage(stage)).toBeLessThan(1)
    }
  })
  it('progressAtEndOfStage is 1 for the last stage', () => {
    expect(progressAtEndOfStage('submitting')).toBe(1)
  })
})

describe('runSubmission', () => {
  it('emits each stage start+end in order, then success, then idle', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    const delay = instant()

    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay }), {
      onPhase: record(phases),
      delay,
      now: () => 1000,
    })

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
    expect(running.every((p) => p.stageStartedAtMs === 1000)).toBe(true)

    const terminal = phases.slice(-2)
    expect(terminal[0].kind).toBe('success')
    expect(terminal[1].kind).toBe('idle')
  })

  it('calls placeOrder exactly once with the payload', async () => {
    const placeOrder = vi.fn()
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: () => {},
      delay: instant(),
    })
    expect(placeOrder).toHaveBeenCalledTimes(1)
    expect(placeOrder).toHaveBeenCalledWith(PAYLOAD)
  })

  it('emits an error phase (plain message, no detail) when a step throws', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn(() => {
      throw new Error('insufficient liquidity')
    })
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: record(phases),
      delay: instant(),
    })
    const terminal = phases[phases.length - 1]
    expect(terminal).toMatchObject({ kind: 'error', message: 'insufficient liquidity' })
    if (terminal.kind === 'error') expect(terminal.detail).toBeUndefined()
    expect(phases.some((p) => p.kind === 'success')).toBe(false)
  })

  it('attaches structured detail when a step throws a DarkPoolError', async () => {
    const phases: SubmissionPhase[] = []
    const steps = buildMockSteps(PAYLOAD, {
      placeOrder: () => {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED, 'slow down', {
          httpStatus: 429,
          retryAfter: '30',
          requestId: 'req-1',
        })
      },
      delay: instant(),
    })
    await runSubmission(steps, { onPhase: record(phases), delay: instant() })
    const terminal = phases[phases.length - 1]
    expect(terminal.kind).toBe('error')
    if (terminal.kind === 'error') {
      expect(terminal.detail?.retryAfter).toBe('30')
      expect(terminal.detail?.requestId).toBe('req-1')
    }
  })

  it('aborts before the next emission when shouldAbort returns true', async () => {
    const phases: SubmissionPhase[] = []
    const placeOrder = vi.fn()
    let count = 0
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder, delay: instant() }), {
      onPhase: record(phases),
      delay: instant(),
      shouldAbort: () => {
        count += 1
        return count > 1
      },
    })
    expect(placeOrder).not.toHaveBeenCalled()
    expect(phases.some((p) => p.kind === 'success' || p.kind === 'error')).toBe(false)
  })

  it('delays each stage by its duration, then SUCCESS_HOLD_MS', async () => {
    const delays: number[] = []
    const delay = async (ms: number) => {
      delays.push(ms)
    }
    await runSubmission(buildMockSteps(PAYLOAD, { placeOrder: vi.fn(), delay }), {
      onPhase: () => {},
      delay,
    })
    expect(delays).toEqual([
      STAGE_DURATIONS_MS.preparing,
      STAGE_DURATIONS_MS.proving,
      STAGE_DURATIONS_MS.encrypting,
      STAGE_DURATIONS_MS.submitting,
      SUCCESS_HOLD_MS,
    ])
  })
})
