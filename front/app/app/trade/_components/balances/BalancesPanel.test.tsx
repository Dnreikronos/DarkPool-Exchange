// Presentational tests: the panel renders purely from useBalances()'s
// status + balances. Source-selection logic lives in useBalances.test.ts.
import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockResult = {
  wallet: { weth: '0', usdc: '0' },
  internal: { weth: '0', usdc: '0' },
  status: 'disconnected' as 'disconnected' | 'loading' | 'error' | 'ready',
  refetch: vi.fn(),
}
vi.mock('../../_hooks/balances/useBalances', () => ({ useBalances: () => mockResult }))

import { BalancesPanel } from './BalancesPanel'

function render(): string {
  return renderToStaticMarkup(<BalancesPanel />)
}

beforeEach(() => {
  Object.assign(mockResult, {
    wallet: { weth: '0', usdc: '0' },
    internal: { weth: '0', usdc: '0' },
    status: 'disconnected',
  })
})

describe('BalancesPanel', () => {
  it('renders the bracketed-tag header in every state', () => {
    expect(render()).toContain('[ BALANCES ]')
  })

  it('disconnected → connect prompt, no balance columns', () => {
    mockResult.status = 'disconnected'
    const html = render()
    expect(html).toContain('[ CONNECT WALLET ]')
    expect(html).not.toContain('[ WALLET ]')
    expect(html).not.toContain('[ DARKPOOL ]')
  })

  it('loading → skeleton', () => {
    mockResult.status = 'loading'
    expect(render()).toContain('Loading balances')
  })

  it('error → unavailable label', () => {
    mockResult.status = 'error'
    expect(render()).toContain('[ BALANCES UNAVAILABLE ]')
  })

  it('ready → both columns with formatted balances', () => {
    mockResult.status = 'ready'
    mockResult.wallet = { weth: '1', usdc: '1000' }
    mockResult.internal = { weth: '0', usdc: '0' }
    const html = render()
    expect(html).toContain('[ WALLET ]')
    expect(html).toContain('[ DARKPOOL ]')
    expect(html).toContain('WETH')
    expect(html).toContain('USDC')
    expect(html).toContain('1.0000') // WETH wallet, 4dp
    expect(html).toContain('1000.00') // USDC wallet, 2dp
    expect(html).toContain('0.0000') // WETH internal, 4dp
    expect(html).toContain('0.00') // USDC internal, 2dp
  })
})
