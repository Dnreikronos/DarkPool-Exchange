// @vitest-environment jsdom

// Locks the Countdown live-region contract (#80): the per-second tick is
// NOT announced (no aria-live on the visible bar); the single sr-only
// role="status" region only changes on meaningful transitions (waiting →
// counting, LIVE ↔ DELAYED). A regression here means screen readers get
// spammed every second — do not loosen these assertions.

import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { Countdown } from './Countdown'

describe('Countdown live region', () => {
  afterEach(cleanup)

  it('keeps aria-live off the ticking label', () => {
    const { container } = render(
      <Countdown latestAuctionUnixSeconds={1_700_000_000n} nowUnixSeconds={1_700_000_002} />
    )
    const live = container.querySelectorAll('[aria-live], [role="status"]')
    expect(live).toHaveLength(1)
    expect(live[0].className).toContain('sr-only')
  })

  it('does not change the announcement between second ticks', () => {
    const { rerender } = render(
      <Countdown
        latestAuctionUnixSeconds={1_700_000_000n}
        nowUnixSeconds={1_700_000_001}
        status="live"
      />
    )
    const before = screen.getByRole('status').textContent
    rerender(
      <Countdown
        latestAuctionUnixSeconds={1_700_000_000n}
        nowUnixSeconds={1_700_000_002}
        status="live"
      />
    )
    expect(screen.getByRole('status').textContent).toBe(before)
  })

  it('changes the announcement when the feed degrades', () => {
    const { rerender } = render(
      <Countdown
        latestAuctionUnixSeconds={1_700_000_000n}
        nowUnixSeconds={1_700_000_001}
        status="live"
      />
    )
    const before = screen.getByRole('status').textContent
    rerender(
      <Countdown
        latestAuctionUnixSeconds={1_700_000_000n}
        nowUnixSeconds={1_700_000_001}
        status="degraded"
      />
    )
    expect(screen.getByRole('status').textContent).not.toBe(before)
    expect(screen.getByRole('status').textContent).toContain('delayed')
  })

  it('changes the announcement when the first auction lands', () => {
    const { rerender } = render(
      <Countdown latestAuctionUnixSeconds={null} nowUnixSeconds={1_700_000_001} />
    )
    expect(screen.getByRole('status').textContent).toContain('Waiting')
    rerender(<Countdown latestAuctionUnixSeconds={1_700_000_000n} nowUnixSeconds={1_700_000_001} />)
    expect(screen.getByRole('status').textContent).toContain('countdown running')
  })

  it('keeps the visible label readable on demand (not aria-hidden)', () => {
    render(<Countdown latestAuctionUnixSeconds={1_700_000_000n} nowUnixSeconds={1_700_000_002} />)
    const label = screen.getByText(/NEXT AUCTION IN/)
    expect(label.closest('[aria-hidden="true"]')).toBeNull()
  })
})
