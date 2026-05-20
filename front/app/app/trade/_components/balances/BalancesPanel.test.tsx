// Smoke tests for the panel composition. We render to static markup
// (no DOM needed) because the project's vitest setup is node-only —
// the existing pattern is pure unit tests + Ladle stories for visual
// review. These tests lock the wallet-store contract end-to-end:
// disconnect → empty state, connect → both balance columns surface.

import * as React from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { walletStore } from '@/lib/wallet/mock-store'

import { BalancesPanel } from './BalancesPanel'

function renderPanel(): string {
  return renderToStaticMarkup(<BalancesPanel />)
}

describe('BalancesPanel', () => {
  beforeEach(() => {
    walletStore.disconnect()
  })

  afterEach(() => {
    walletStore.disconnect()
  })

  it('renders the bracketed-tag header in every state', () => {
    expect(renderPanel()).toContain('[ BALANCES ]')
  })

  it('renders the connect-wallet empty state when disconnected', () => {
    const html = renderPanel()
    expect(html).toContain('[ CONNECT WALLET ]')
    // No balance columns surface in the empty state.
    expect(html).not.toContain('[ WALLET ]')
    expect(html).not.toContain('[ DARKPOOL ]')
  })

  it('renders both columns and both tokens when connected', () => {
    walletStore.connect()
    const html = renderPanel()
    expect(html).toContain('[ WALLET ]')
    expect(html).toContain('[ DARKPOOL ]')
    expect(html).toContain('WETH')
    expect(html).toContain('USDC')
    // F1.3 seeds wallet at (1 WETH, 1000 USDC) and internal at (0, 0).
    expect(html).toContain('1.0000') // WETH wallet (4dp)
    // 1000 USDC sits below the 10,000 thousands threshold — no comma.
    expect(html).toContain('1000.00')
    expect(html).toContain('0.0000') // WETH internal (4dp)
    expect(html).toContain('0.00') // USDC internal (2dp)
  })

  // The wallet store has no balance-mutation API in Phase 1 — only
  // connect/disconnect. Until F1.5 (#72) lands deposit/withdraw, the
  // closest end-to-end test of the reactive read contract is: render,
  // mutate via connect, render again, see the output transition. The
  // subscriber-notification half of "auto-update" is locked by the
  // walletStore tests (#70).
  it('reflects subsequent store mutations on re-render', () => {
    const before = renderPanel()
    expect(before).toContain('[ CONNECT WALLET ]')
    expect(before).not.toContain('1.0000')

    walletStore.connect()

    const after = renderPanel()
    expect(after).not.toContain('[ CONNECT WALLET ]')
    expect(after).toContain('1.0000')

    walletStore.disconnect()

    const restored = renderPanel()
    expect(restored).toContain('[ CONNECT WALLET ]')
    expect(restored).not.toContain('1.0000')
  })

  it('does not introduce a lime accent anywhere on the panel surface', () => {
    walletStore.connect()
    const html = renderPanel()
    // The auction countdown owns the /trade lime budget; this panel
    // must not claim it via any class (text, bg, border, ring, shadow).
    expect(html).not.toMatch(/brand-accent/)
    expect(html).not.toMatch(/accent-glow/)
  })
})
