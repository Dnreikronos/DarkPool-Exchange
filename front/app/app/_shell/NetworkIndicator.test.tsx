// @vitest-environment jsdom

// The banner's network chip used to be static copy — it announced
// "Arbitrum, chain 42161, status offline" no matter which chain the
// wallet was actually on, and no matter whether one was connected
// (#205 item 4). Both the visible label and the accessible name have to
// track real chain state now that wagmi provides it.

import * as React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

const useAccountMock = vi.fn()

vi.mock('wagmi', () => ({
  useAccount: () => useAccountMock(),
}))

vi.mock('@/lib/wallet', () => ({
  targetChain: { id: 31337, name: 'Foundry' },
}))

import { NetworkIndicator } from './NetworkIndicator'

afterEach(() => {
  cleanup()
  useAccountMock.mockReset()
})

function label(): string {
  return screen.getByRole('status').getAttribute('aria-label') ?? ''
}

describe('NetworkIndicator', () => {
  it('names the target chain and reports disconnected when no wallet is connected', () => {
    useAccountMock.mockReturnValue({ isConnected: false, chain: undefined })
    render(<NetworkIndicator />)
    expect(label()).toBe('Network: Foundry, chain 31337, status disconnected')
    // Casing is the `uppercase` utility's job; jsdom applies no CSS, so
    // assert the text the component actually renders.
    expect(screen.getByRole('status').textContent).toContain('Foundry · 31337')
  })

  it('reports the connected chain when it matches the target', () => {
    useAccountMock.mockReturnValue({
      isConnected: true,
      chain: { id: 31337, name: 'Foundry' },
    })
    render(<NetworkIndicator />)
    expect(label()).toBe('Network: Foundry, chain 31337, status connected')
  })

  it('reports the wallet chain, not the target, on a network mismatch', () => {
    useAccountMock.mockReturnValue({
      isConnected: true,
      chain: { id: 42161, name: 'Arbitrum One' },
    })
    render(<NetworkIndicator />)
    expect(label()).toBe('Network: Arbitrum One, chain 42161, status wrong network')
    expect(screen.getByRole('status').textContent).toContain('Arbitrum One · 42161')
  })

  it('handles a connected chain wagmi does not recognise', () => {
    useAccountMock.mockReturnValue({ isConnected: true, chain: undefined })
    render(<NetworkIndicator />)
    expect(label()).toBe('Network: unrecognised, chain unknown, status wrong network')
  })
})
