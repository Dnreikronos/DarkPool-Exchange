// @vitest-environment jsdom

// Locks the ARIA tabs keyboard contract on the BUY/SELL segmented
// control (#80): roving tabindex (one Tab stop), arrows move the
// selection, Home/End jump to first/last.

import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import type { OrderSide } from '../../_lib/entry/validate'
import { BuySellTabs } from './BuySellTabs'

function Harness({ initial = 'buy' as OrderSide }) {
  const [side, setSide] = React.useState<OrderSide>(initial)
  return <BuySellTabs value={side} onChange={setSide} />
}

const tab = (name: string) => screen.getByRole('tab', { name })

describe('BuySellTabs keyboard', () => {
  afterEach(cleanup)

  it('exposes a single Tab stop via roving tabindex', () => {
    render(<Harness />)
    expect(tab('[ BUY ]').tabIndex).toBe(0)
    expect(tab('[ SELL ]').tabIndex).toBe(-1)
  })

  it('moves selection and focus with ArrowRight / ArrowLeft', () => {
    render(<Harness />)
    tab('[ BUY ]').focus()
    fireEvent.keyDown(tab('[ BUY ]'), { key: 'ArrowRight' })
    expect(tab('[ SELL ]').getAttribute('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(tab('[ SELL ]'))

    fireEvent.keyDown(tab('[ SELL ]'), { key: 'ArrowLeft' })
    expect(tab('[ BUY ]').getAttribute('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(tab('[ BUY ]'))
  })

  it('jumps to first/last with Home / End', () => {
    render(<Harness />)
    tab('[ BUY ]').focus()
    fireEvent.keyDown(tab('[ BUY ]'), { key: 'End' })
    expect(tab('[ SELL ]').getAttribute('aria-selected')).toBe('true')

    fireEvent.keyDown(tab('[ SELL ]'), { key: 'Home' })
    expect(tab('[ BUY ]').getAttribute('aria-selected')).toBe('true')
  })

  it('ignores arrows while disabled', () => {
    render(<BuySellTabs value="buy" onChange={() => {}} disabled />)
    fireEvent.keyDown(tab('[ BUY ]'), { key: 'ArrowRight' })
    expect(tab('[ BUY ]').getAttribute('aria-selected')).toBe('true')
    expect(tab('[ SELL ]').getAttribute('aria-selected')).toBe('false')
  })
})
