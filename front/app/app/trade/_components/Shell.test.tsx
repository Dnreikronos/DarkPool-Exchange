// @vitest-environment jsdom

// Shell wiring tests (#207). The /trade Shell must mount the REAL panels
// (OrderBook, charts, OrderEntry, Tape, MyOrders) — not the F1.x
// placeholder shells — and wire click-to-fill from the book into the
// order-entry form. Rendered with the same provider sandwich the trading
// layout provides (QueryClientProvider + DarkPoolClientProvider) and a
// stub client returning a populated book + auction history.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

import type { DarkPoolClient } from '@/lib/sdk/client'
import { createFactoryContext, mockAuctionSummary, mockOrderBook } from '@/lib/sdk/mocks'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import {
  CancelOrderResponseSchema,
  GetAuctionHistoryResponseSchema,
  GetOrderResponseSchema,
  OrderInfoSchema,
  PlaceOrderResponseSchema,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'

import { Shell } from './Shell'

// useRealSubmission needs a Web Worker (prover) — not available in jsdom.
// NEXT_PUBLIC_USE_MOCKS='true' in vitest env means the real path is never
// taken; stub it like OrderEntry.test.tsx does.
vi.mock('../_hooks/entry/useRealSubmission', () => ({
  useRealSubmission: () => ({
    buildSteps: () => [],
    provingPct: null,
  }),
}))

// Every placeholder string the pre-#207 Shell rendered. None may survive.
const PLACEHOLDER_COPY = [
  'NO DATA · F1.6',
  'CHART · F1.8',
  'AWAITING WALLET · F1.9',
  'NO AUCTIONS YET · F1.7',
  'AWAITING DATA',
  'Awaiting wallet connection and order entry form (F1.9).',
]

function buildClient(): DarkPoolClient {
  const ctx = createFactoryContext({ seed: 7 })
  const book = mockOrderBook(ctx, { depth: 6 })
  const auctions = [
    mockAuctionSummary(ctx, { clearingPrice: '2418.10', timestampUnix: 1700000010n }),
    mockAuctionSummary(ctx, { clearingPrice: '2405.76', timestampUnix: 1700000005n }),
  ]
  return {
    async placeOrder() {
      return create(PlaceOrderResponseSchema, { order: create(OrderInfoSchema, {}) })
    },
    async cancelOrder() {
      return create(CancelOrderResponseSchema, {})
    },
    async getOrder() {
      return create(GetOrderResponseSchema, { order: create(OrderInfoSchema, {}) })
    },
    async getOrderBook() {
      return book
    },
    async getAuctionHistory() {
      return create(GetAuctionHistoryResponseSchema, { auctions })
    },
    async *streamAuctions() {
      // Hold the stream open without emitting.
      await new Promise(() => {})
    },
  }
}

function renderShell() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <DarkPoolClientProvider client={buildClient()}>
        <Shell />
      </DarkPoolClientProvider>
    </QueryClientProvider>
  )
}

async function waitForBook() {
  await waitFor(() => {
    if (screen.queryAllByText('[ BIDS ]').length === 0) {
      throw new Error('depth table not loaded yet')
    }
  })
}

beforeAll(() => {
  // jsdom has no ResizeObserver; visx ParentSize (DepthChart) and
  // lightweight-charts need the constructor to exist.
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', ResizeObserverStub)
})

describe('Shell wiring (#207)', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('renders no placeholder copy once the panels load', async () => {
    renderShell()
    await waitForBook()
    const text = document.body.textContent ?? ''
    for (const copy of PLACEHOLDER_COPY) {
      expect(text).not.toContain(copy)
    }
  })

  it('mounts the real panels: orderbook, charts, order entry, tape, my orders', async () => {
    renderShell()
    await waitForBook()
    // Orderbook: populated depth table (desktop + mobile instances).
    expect(screen.getAllByText('[ BIDS ]').length).toBeGreaterThan(0)
    // Charts: depth + clearing-price headers.
    expect(screen.getAllByText('[ DEPTH ]').length).toBeGreaterThan(0)
    expect(screen.getAllByText('[ CLEARING PRICE ]').length).toBeGreaterThan(0)
    // Order entry: real form with side tabs.
    expect(screen.getAllByRole('tab', { name: '[ BUY ]' }).length).toBeGreaterThan(0)
    // Tape: countdown bar fed by the auction history.
    expect(screen.getAllByText(/NEXT AUCTION IN|WAITING FOR FIRST AUCTION/).length).toBeGreaterThan(
      0
    )
    // My orders survives the rewire.
    expect(screen.getAllByText('[ MY ORDERS ]').length).toBeGreaterThan(0)
  })

  it('click-to-fill: clicking a bid level fills the order-entry price input', async () => {
    renderShell()
    await waitForBook()
    const [bidButton] = screen.getAllByRole('button', { name: /^Bid / })
    const price = (bidButton.getAttribute('aria-label') ?? '').replace('Bid ', '')
    expect(price).not.toBe('')
    fireEvent.click(bidButton)
    const priceInput = screen.getByLabelText('[ PRICE · USDC ]') as HTMLInputElement
    expect(priceInput.value).toBe(price)
  })

  it('click-to-fill: clicking an ask level switches the form to SELL', async () => {
    renderShell()
    await waitForBook()
    const [askButton] = screen.getAllByRole('button', { name: /^Ask / })
    fireEvent.click(askButton)
    const sellTabs = screen.getAllByRole('tab', { name: '[ SELL ]' })
    // The desktop form's SELL tab is now the selected one.
    expect(sellTabs.some((tab) => tab.getAttribute('aria-selected') === 'true')).toBe(true)
  })

  it('mounts the real order-entry form inside the mobile sheet', async () => {
    renderShell()
    await waitForBook()
    fireEvent.click(screen.getByRole('button', { name: 'Open order entry' }))
    // Desktop panel + open sheet → two real forms, each with its own inputs.
    await waitFor(() => {
      if (screen.getAllByLabelText('[ PRICE · USDC ]').length < 2) {
        throw new Error('sheet form not mounted yet')
      }
    })
    expect(document.body.textContent ?? '').not.toContain(
      'Awaiting wallet connection and order entry form (F1.9).'
    )
  })

  // The Shell renders every panel twice — once per layout — and both trees
  // stay in the DOM because the split is CSS-only. Any static DOM id inside
  // a panel therefore collides, silently repointing aria-labelledby /
  // aria-controls at the first match in document order. The axe suite in
  // Shell.a11y.test.tsx does NOT catch this: its WCAG-tag filter excludes
  // the duplicate-id rules. Assert it directly instead.
  it('emits no duplicate DOM ids across the desktop and mobile layouts', async () => {
    renderShell()
    await waitForBook()
    fireEvent.click(screen.getByRole('button', { name: 'Open order entry' }))
    await waitFor(() => {
      if (screen.getAllByLabelText('[ PRICE · USDC ]').length < 2) {
        throw new Error('sheet form not mounted yet')
      }
    })

    const counts = new Map<string, number>()
    for (const el of document.querySelectorAll('[id]')) {
      counts.set(el.id, (counts.get(el.id) ?? 0) + 1)
    }
    const duplicates = [...counts.entries()].filter(([, n]) => n > 1).map(([id]) => id)
    expect(duplicates).toEqual([])
  })
})
