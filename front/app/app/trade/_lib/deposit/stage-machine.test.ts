import { describe, expect, it } from 'vitest'
import { INITIAL_STAGE, isInFlight, isTerminal, reduceStage, type Stage } from './stage-machine'

describe('reduceStage', () => {
  it('starts in idle', () => {
    expect(INITIAL_STAGE).toEqual({ kind: 'idle' })
  })

  it('deposit flow: idle -> approving -> submitting -> confirmed', () => {
    let s: Stage = INITIAL_STAGE
    s = reduceStage(s, { type: 'start', needsApproval: true })
    expect(s).toEqual({ kind: 'approving' })
    s = reduceStage(s, { type: 'approvalDone' })
    expect(s).toEqual({ kind: 'submitting' })
    s = reduceStage(s, { type: 'submitted' })
    expect(s).toEqual({ kind: 'confirmed' })
  })

  it('skips approving when allowance is already sufficient', () => {
    const s = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: false })
    expect(s).toEqual({ kind: 'submitting' })
  })

  it('fail during approving transitions to error', () => {
    const start = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: true })
    const failed = reduceStage(start, { type: 'fail', message: 'Approve reverted.' })
    expect(failed).toEqual({ kind: 'error', errorMessage: 'Approve reverted.' })
  })

  it('fail during submitting transitions to error', () => {
    const submitting = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: false })
    const failed = reduceStage(submitting, { type: 'fail', message: 'Deposit reverted.' })
    expect(failed).toEqual({ kind: 'error', errorMessage: 'Deposit reverted.' })
  })

  it('reset returns to idle from any stage', () => {
    const stages: Stage[] = [
      { kind: 'idle' },
      { kind: 'approving' },
      { kind: 'submitting' },
      { kind: 'confirmed' },
      { kind: 'error', errorMessage: 'x' },
    ]
    for (const s of stages) {
      expect(reduceStage(s, { type: 'reset' })).toEqual(INITIAL_STAGE)
    }
  })

  it('start is a no-op while in flight', () => {
    const approving: Stage = { kind: 'approving' }
    expect(reduceStage(approving, { type: 'start', needsApproval: true })).toEqual(approving)
    const submitting: Stage = { kind: 'submitting' }
    expect(reduceStage(submitting, { type: 'start', needsApproval: false })).toEqual(submitting)
  })

  it('start re-enters from an error state (retry)', () => {
    const errored: Stage = { kind: 'error', errorMessage: 'previous failure' }
    const restarted = reduceStage(errored, { type: 'start', needsApproval: true })
    expect(restarted).toEqual({ kind: 'approving' })
  })

  it('approvalDone is ignored if not currently approving', () => {
    const submitting: Stage = { kind: 'submitting' }
    expect(reduceStage(submitting, { type: 'approvalDone' })).toEqual(submitting)
  })

  it('submitted is ignored if not currently submitting', () => {
    const approving: Stage = { kind: 'approving' }
    expect(reduceStage(approving, { type: 'submitted' })).toEqual(approving)
  })

  it('fail is ignored when terminal (stale timer cannot reopen)', () => {
    const confirmed: Stage = { kind: 'confirmed' }
    expect(reduceStage(confirmed, { type: 'fail', message: 'x' })).toEqual(confirmed)
    const errored: Stage = { kind: 'error', errorMessage: 'prev' }
    expect(reduceStage(errored, { type: 'fail', message: 'x' })).toEqual(errored)
  })

  it('fail is ignored from idle (defence against bad call order)', () => {
    expect(reduceStage(INITIAL_STAGE, { type: 'fail', message: 'x' })).toEqual(INITIAL_STAGE)
  })
})

describe('isInFlight / isTerminal', () => {
  it('classifies stages correctly', () => {
    expect(isInFlight({ kind: 'idle' })).toBe(false)
    expect(isInFlight({ kind: 'approving' })).toBe(true)
    expect(isInFlight({ kind: 'submitting' })).toBe(true)
    expect(isInFlight({ kind: 'confirmed' })).toBe(false)
    expect(isInFlight({ kind: 'error', errorMessage: 'x' })).toBe(false)

    expect(isTerminal({ kind: 'idle' })).toBe(false)
    expect(isTerminal({ kind: 'approving' })).toBe(false)
    expect(isTerminal({ kind: 'submitting' })).toBe(false)
    expect(isTerminal({ kind: 'confirmed' })).toBe(true)
    expect(isTerminal({ kind: 'error', errorMessage: 'x' })).toBe(true)
  })
})
