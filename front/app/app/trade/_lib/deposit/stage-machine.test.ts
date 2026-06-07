import { describe, expect, it } from 'vitest'
import { INITIAL_STAGE, isInFlight, isTerminal, reduceStage, type Stage } from './stage-machine'

describe('reduceStage', () => {
  it('starts in idle', () => {
    expect(INITIAL_STAGE).toEqual({ kind: 'idle' })
  })

  it('deposit flow: idle -> approving -> submitting -> confirmed, signing then mining per step', () => {
    let s: Stage = INITIAL_STAGE
    s = reduceStage(s, { type: 'start', needsApproval: true })
    expect(s).toEqual({ kind: 'approving', phase: 'signing' })
    s = reduceStage(s, { type: 'signed' })
    expect(s).toEqual({ kind: 'approving', phase: 'mining' })
    s = reduceStage(s, { type: 'approvalDone' })
    expect(s).toEqual({ kind: 'submitting', phase: 'signing' })
    s = reduceStage(s, { type: 'signed' })
    expect(s).toEqual({ kind: 'submitting', phase: 'mining' })
    s = reduceStage(s, { type: 'submitted' })
    expect(s).toEqual({ kind: 'confirmed' })
  })

  it('skips approving when allowance is already sufficient (starts in submitting/signing)', () => {
    const s = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: false })
    expect(s).toEqual({ kind: 'submitting', phase: 'signing' })
  })

  it('signed only advances a signing in-flight stage to mining', () => {
    // idle / confirmed / error ignore signed
    expect(reduceStage(INITIAL_STAGE, { type: 'signed' })).toEqual(INITIAL_STAGE)
    const confirmed: Stage = { kind: 'confirmed' }
    expect(reduceStage(confirmed, { type: 'signed' })).toEqual(confirmed)
    // already mining -> idempotent
    const mining: Stage = { kind: 'submitting', phase: 'mining' }
    expect(reduceStage(mining, { type: 'signed' })).toEqual(mining)
  })

  it('fail during approving transitions to error', () => {
    const start = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: true })
    const failed = reduceStage(start, { type: 'fail', message: 'Approve reverted.' })
    expect(failed).toEqual({ kind: 'error', errorMessage: 'Approve reverted.' })
  })

  it('fail during submitting (mining) transitions to error', () => {
    let s = reduceStage(INITIAL_STAGE, { type: 'start', needsApproval: false })
    s = reduceStage(s, { type: 'signed' })
    const failed = reduceStage(s, { type: 'fail', message: 'Deposit reverted.' })
    expect(failed).toEqual({ kind: 'error', errorMessage: 'Deposit reverted.' })
  })

  it('reset returns to idle from any stage', () => {
    const stages: Stage[] = [
      { kind: 'idle' },
      { kind: 'approving', phase: 'signing' },
      { kind: 'submitting', phase: 'mining' },
      { kind: 'confirmed' },
      { kind: 'error', errorMessage: 'x' },
    ]
    for (const s of stages) {
      expect(reduceStage(s, { type: 'reset' })).toEqual(INITIAL_STAGE)
    }
  })

  it('start is a no-op while in flight', () => {
    const approving: Stage = { kind: 'approving', phase: 'signing' }
    expect(reduceStage(approving, { type: 'start', needsApproval: true })).toEqual(approving)
    const submitting: Stage = { kind: 'submitting', phase: 'mining' }
    expect(reduceStage(submitting, { type: 'start', needsApproval: false })).toEqual(submitting)
  })

  it('start re-enters from an error state (retry)', () => {
    const errored: Stage = { kind: 'error', errorMessage: 'previous failure' }
    const restarted = reduceStage(errored, { type: 'start', needsApproval: true })
    expect(restarted).toEqual({ kind: 'approving', phase: 'signing' })
  })

  it('approvalDone is ignored if not currently approving', () => {
    const submitting: Stage = { kind: 'submitting', phase: 'signing' }
    expect(reduceStage(submitting, { type: 'approvalDone' })).toEqual(submitting)
  })

  it('submitted is ignored if not currently submitting', () => {
    const approving: Stage = { kind: 'approving', phase: 'mining' }
    expect(reduceStage(approving, { type: 'submitted' })).toEqual(approving)
  })

  it('fail is ignored when terminal (stale callback cannot reopen)', () => {
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
    expect(isInFlight({ kind: 'approving', phase: 'signing' })).toBe(true)
    expect(isInFlight({ kind: 'submitting', phase: 'mining' })).toBe(true)
    expect(isInFlight({ kind: 'confirmed' })).toBe(false)
    expect(isInFlight({ kind: 'error', errorMessage: 'x' })).toBe(false)

    expect(isTerminal({ kind: 'idle' })).toBe(false)
    expect(isTerminal({ kind: 'approving', phase: 'signing' })).toBe(false)
    expect(isTerminal({ kind: 'submitting', phase: 'mining' })).toBe(false)
    expect(isTerminal({ kind: 'confirmed' })).toBe(true)
    expect(isTerminal({ kind: 'error', errorMessage: 'x' })).toBe(true)
  })
})
