// @vitest-environment jsdom

// A clickable depth row is a <button> carrying an aria-label, and an
// aria-label REPLACES the element's content as its accessible name. The
// size and total columns are rendered inside the button, so a label that
// names only the price makes them unreachable to screen readers even
// though sighted users read them off the same row (#205 item 5).

import * as React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { DepthRow as DepthRowData } from '../../_lib/orderbook/depth'
import { DepthRow } from './DepthRow'

const row: DepthRowData = {
  level: { price: '2418.10', totalSize: '3.2500' } as DepthRowData['level'],
  cumulative: '11.7500',
  barFraction: 0.4,
}

afterEach(cleanup)

describe('DepthRow accessible name', () => {
  it('names price, size and total on a clickable bid', () => {
    render(<DepthRow row={row} side={Side.BUY} onSelect={vi.fn()} />)
    const button = screen.getByRole('button')
    const name = button.getAttribute('aria-label') ?? ''
    expect(name).toContain('2418.10')
    expect(name).toContain('3.2500')
    expect(name).toContain('11.7500')
  })

  it('names the side so bids and asks are distinguishable', () => {
    render(<DepthRow row={row} side={Side.SELL} onSelect={vi.fn()} />)
    expect(screen.getByRole('button').getAttribute('aria-label')).toMatch(/^Ask /)
  })

  it('leaves a non-clickable row unlabelled so its columns are read directly', () => {
    const { container } = render(<DepthRow row={row} side={Side.BUY} />)
    expect(container.querySelector('[aria-label]')).toBeNull()
  })
})
