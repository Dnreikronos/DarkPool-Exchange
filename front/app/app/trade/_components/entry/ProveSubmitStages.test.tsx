// @vitest-environment jsdom

// Locks the PlaceButton announcement contract (#80): the visible label
// ticks every 100 ms with elapsed seconds during the 5–30 s proving
// stage, so it must NOT be a live region. The single sr-only
// role="status" announces stage transitions only (no timer), and errors
// stay out of it — <SubmitError>'s role="alert" owns those.

import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import type { SubmissionPhase } from '../../_hooks/entry/useSubmitStages'
import { PlaceButton, SubmitError } from './ProveSubmitStages'

function renderButton(phase: SubmissionPhase, now = () => 1_700_000_000_000) {
  return render(
    <PlaceButton idleLabel="BUY · WETH" phase={phase} onClick={() => {}} accent={false} now={now} />
  )
}

const proving = (startedAtMs?: number): SubmissionPhase => ({
  kind: 'running',
  stage: 'proving',
  progress: 0.4,
  stageStartedAtMs: startedAtMs,
})

describe('PlaceButton announcements', () => {
  afterEach(cleanup)

  it('keeps aria-live off the button (its label ticks every 100ms)', () => {
    const { container } = renderButton(proving(1_700_000_000_000))
    const button = container.querySelector('button')
    expect(button?.getAttribute('aria-live')).toBeNull()
    expect(button?.getAttribute('aria-busy')).toBe('true')
  })

  it('announces the stage without the elapsed timer', () => {
    renderButton(proving(1_699_999_990_000), () => 1_700_000_000_000)
    // Visible label carries the timer; the live region must not.
    expect(screen.getByRole('button').textContent).toContain('10.0s')
    const status = screen.getByRole('status')
    expect(status.textContent).toContain('GENERATING PROOF')
    expect(status.textContent).not.toMatch(/\d+\.\ds/)
  })

  it('does not change the announcement as elapsed time advances', () => {
    const { rerender } = renderButton(proving(1_699_999_990_000), () => 1_700_000_000_000)
    const before = screen.getByRole('status').textContent
    rerender(
      <PlaceButton
        idleLabel="BUY · WETH"
        phase={proving(1_699_999_990_000)}
        onClick={() => {}}
        accent={false}
        now={() => 1_700_000_005_000}
      />
    )
    expect(screen.getByRole('status').textContent).toBe(before)
  })

  it('announces success and stays silent when idle', () => {
    const { rerender } = renderButton({ kind: 'idle' })
    expect(screen.getByRole('status').textContent).toBe('')
    rerender(
      <PlaceButton
        idleLabel="BUY · WETH"
        phase={{ kind: 'success' }}
        onClick={() => {}}
        accent={false}
      />
    )
    expect(screen.getByRole('status').textContent).toBe('ORDER PLACED')
  })

  it('leaves error announcements to SubmitError role="alert"', () => {
    renderButton({ kind: 'error', message: 'Order rejected' })
    expect(screen.getByRole('status').textContent).toBe('')

    render(<SubmitError phase={{ kind: 'error', message: 'Order rejected' }} />)
    expect(screen.getByRole('alert').textContent).toContain('Order rejected')
  })
})
