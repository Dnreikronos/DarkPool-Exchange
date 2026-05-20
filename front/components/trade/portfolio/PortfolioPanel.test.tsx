// Smoke tests for the portfolio surface. Mirrors the BalancesPanel
// pattern: SSR-only static markup, no DOM. Locks the wallet-store +
// mock-store contracts end-to-end (disconnected → fill table + empty
// state; connected → P&L card + fill table) so the panel survives the
// next mock-store refactor.

import * as React from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { createMockStore } from '../../../lib/mock-store'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '../../../lib/wallet/mock-store'

import { PortfolioPanel } from './PortfolioPanel'

function render(): string {
  return renderToStaticMarkup(<PortfolioPanel />)
}

describe('PortfolioPanel', () => {
  beforeEach(() => {
    walletStore.disconnect()
  })

  afterEach(() => {
    walletStore.disconnect()
  })

  it('renders the page header in every state', () => {
    expect(render()).toContain('[ PORTFOLIO · ETH / USDC ]')
    expect(render()).toContain('POSITIONS')
  })

  it('renders a connect prompt when disconnected', () => {
    // React encodes `&` as `&amp;` in SSR HTML.
    expect(render()).toContain('[ CONNECT WALLET TO SEE POSITION + P&amp;L ]')
  })

  it('renders the P&L stat triplet when connected', () => {
    walletStore.connect()
    const html = render()
    expect(html).toContain('WETH POSITION')
    expect(html).toContain('USDC DELTA')
    expect(html).toContain('UNREALIZED P&amp;L')
  })

  it('renders the fill-history empty state when no fills exist', () => {
    const html = render()
    expect(html).toContain('[ NO FILLS YET')
    expect(html).toContain('[ FILL HISTORY · 00 ]')
  })

  it('disables the CSV export button when there are no fills', () => {
    expect(render()).toMatch(/disabled[^>]*>?\s*\[ EXPORT CSV \]/)
  })
})

describe('createMockStore + portfolio derivations (integration)', () => {
  // Hits the same selectors the hook does, against a real store, to lock
  // the wire contract between mock-store fills and the panel's numbers.
  it('derives a position and P&L summary from store fills', async () => {
    const store = createMockStore({ seed: 1, now: () => 1_700_000_000 })
    store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    const auction = store.getState().runAuction()
    const fills = store.getState().fillHistory
    expect(fills).toHaveLength(1)

    const { computeSummary } = await import('./pnl')
    const summary = computeSummary(fills, auction.clearingPrice)
    expect(summary.position.weth).toBe('1')
    // The mock auction records fills at the clearing price, not the
    // posted order price — so the single-fill avgEntry equals the
    // auction's clearingPrice.
    expect(summary.avgEntry).toBe(fills[0].price)
    expect(summary.mark).toBe(auction.clearingPrice)
  })
})
