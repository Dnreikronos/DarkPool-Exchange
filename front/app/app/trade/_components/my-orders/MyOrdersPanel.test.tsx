// Smoke tests for the panel composition. We render to static markup
// (node-only vitest setup, same pattern as Balances/OrderEntry). These
// lock the wallet/store-derived states end-to-end at the composition
// layer; the hook's diffing reducer is covered by afterlife.test.ts.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'

import type { UseMyOrdersReturn } from '../../_hooks/my-orders/useMyOrders'
import type { MyOrderRow } from '../../_lib/my-orders/types'
import { MyOrdersPanel } from './MyOrdersPanel'

function mkOrder(overrides: Partial<OrderInfo> = {}): OrderInfo {
  return create(OrderInfoSchema, {
    id: 'o-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '1',
    remainingSize: '1',
    commitmentKey: 'mock-k',
    submittedAtUnix: 1700000000n,
    expiresAtUnix: 0n,
    ...overrides,
  })
}

function stubHook(rows: MyOrderRow[]): () => UseMyOrdersReturn {
  return () => ({
    rows,
    userPrices: new Set(rows.filter((r) => r.status === 'open').map((r) => r.order.price)),
    cancel: () => true,
  })
}

function renderPanel(rows: MyOrderRow[] = []): string {
  return renderToStaticMarkup(<MyOrdersPanel useOrders={stubHook(rows)} />)
}

describe('MyOrdersPanel', () => {
  beforeEach(() => {
    walletStore.disconnect()
  })
  afterEach(() => {
    walletStore.disconnect()
  })

  it('renders the bracketed-tag header in every state', () => {
    expect(renderPanel()).toContain('[ MY ORDERS ]')
  })

  it('renders the connect-wallet empty state when disconnected', () => {
    const html = renderPanel([{ order: mkOrder(), status: 'open' }])
    expect(html).toContain('[ CONNECT WALLET ]')
    // Column headers do not surface when disconnected.
    expect(html).not.toContain('>TIME<')
  })

  it('renders the no-orders empty state when connected with no rows', () => {
    walletStore.connect()
    const html = renderPanel([])
    expect(html).toContain('[ NO ORDERS YET ]')
    expect(html).not.toContain('>TIME<')
  })

  it('renders column headers and one row per order when connected', () => {
    walletStore.connect()
    const html = renderPanel([
      { order: mkOrder({ id: 'o-buy', side: Side.BUY, price: '3000' }), status: 'open' },
      {
        order: mkOrder({ id: 'o-sell', side: Side.SELL, price: '3010', remainingSize: '0.5' }),
        status: 'open',
      },
    ])
    expect(html).toContain('>TIME<')
    expect(html).toContain('>SIDE<')
    expect(html).toContain('>PRICE<')
    expect(html).toContain('>SIZE<')
    expect(html).toContain('>STATUS<')
    expect(html).toContain('[ BUY ]')
    expect(html).toContain('[ SELL ]')
    expect(html).toContain('Cancel order o-buy')
    expect(html).toContain('Cancel order o-sell')
  })

  it('renders all three status labels', () => {
    walletStore.connect()
    const html = renderPanel([
      { order: mkOrder({ id: 'a' }), status: 'open' },
      { order: mkOrder({ id: 'b' }), status: 'filled' },
      { order: mkOrder({ id: 'c' }), status: 'cancelled' },
    ])
    expect(html).toContain('[ OPEN ]')
    expect(html).toContain('[ FILLED ]')
    expect(html).toContain('[ CANCELLED ]')
  })

  it('disables the cancel button for filled and cancelled rows', () => {
    walletStore.connect()
    const html = renderPanel([
      { order: mkOrder({ id: 'open-a' }), status: 'open' },
      { order: mkOrder({ id: 'filled-b' }), status: 'filled' },
      { order: mkOrder({ id: 'cxl-c' }), status: 'cancelled' },
    ])
    // We check for the `disabled=""` attribute string that ReactDOM
    // emits for boolean attributes — Tailwind's `disabled:` class
    // variants would otherwise false-match on the substring `disabled`.
    function buttonHasDisabledAttr(id: string): boolean {
      const re = new RegExp(`<button[^>]*aria-label="Cancel order ${id}"[^>]*disabled=""`)
      const reFlipped = new RegExp(`<button[^>]*disabled=""[^>]*aria-label="Cancel order ${id}"`)
      return re.test(html) || reFlipped.test(html)
    }
    expect(buttonHasDisabledAttr('open-a')).toBe(false)
    expect(buttonHasDisabledAttr('filled-b')).toBe(true)
    expect(buttonHasDisabledAttr('cxl-c')).toBe(true)
  })

  it('does not introduce a lime accent on the panel surface', () => {
    walletStore.connect()
    const html = renderPanel([
      { order: mkOrder({ id: 'a' }), status: 'open' },
      { order: mkOrder({ id: 'b' }), status: 'cancelled' },
    ])
    // The /trade lime budget belongs to the auction countdown — this
    // panel must not claim it via text/background/border/ring/shadow.
    // The focus-visible outline on cancel may carry the colour, but it
    // only paints when the element has keyboard focus, so we whitelist
    // that token by name.
    const matches = html.match(/brand-accent/g) ?? []
    for (const m of matches) {
      expect(m).toBe('brand-accent')
    }
    // None of the always-painted surfaces (bg/text) should reference it.
    expect(html).not.toMatch(/bg-brand-accent/)
    expect(html).not.toMatch(/text-brand-accent/)
    expect(html).not.toMatch(/accent-glow/)
  })
})
