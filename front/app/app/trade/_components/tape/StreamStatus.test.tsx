// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { StreamStatus } from './StreamStatus'

afterEach(cleanup)

describe('StreamStatus', () => {
  it('shows a blinking white pill labelled LIVE when live', () => {
    const { container } = render(<StreamStatus status="live" />)
    expect(screen.getByText('LIVE')).toBeTruthy()
    const pill = container.querySelector('span[aria-hidden="true"]')
    expect(pill?.className).toContain('bg-brand-fg')
    expect(pill?.className).toContain('animate-blink')
  })

  it('shows a static muted pill labelled DELAYED when degraded', () => {
    const { container } = render(<StreamStatus status="degraded" />)
    expect(screen.getByText('DELAYED')).toBeTruthy()
    const pill = container.querySelector('span[aria-hidden="true"]')
    expect(pill?.className).toContain('bg-brand-muted')
    expect(pill?.className).not.toContain('animate-blink')
  })

  it('labels the connecting state DELAYED too', () => {
    render(<StreamStatus status="connecting" />)
    expect(screen.getByText('DELAYED')).toBeTruthy()
  })
})
