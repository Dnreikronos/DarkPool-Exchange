// @vitest-environment jsdom

// Automated axe-core scan of the /trade shell (#80, re-baselined for the
// real panels in #207). jsdom applies no stylesheets, so BOTH the desktop
// and mobile layouts are present in the DOM at once — any duplicate-ID
// finding here is real, not an artifact (the desktop layout stays in the
// DOM behind `display:none` in real browsers too). Also scans with the
// mobile order-entry sheet open (Radix portal), which double-mounts the
// order-entry form.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeAll, describe, it, vi } from 'vitest'
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
import { expectNoAxeViolations } from '@/test/axe'

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

describe('Shell a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    renderShell()
    await waitForBook()
    await expectNoAxeViolations()
  })

  it('has no axe violations when connected', async () => {
    walletStore.connect()
    renderShell()
    await waitForBook()
    await expectNoAxeViolations()
  })

  it('has no axe violations with the mobile order-entry sheet open', async () => {
    renderShell()
    await waitForBook()
    fireEvent.click(screen.getByRole('button', { name: 'Open order entry' }))
    await expectNoAxeViolations()
  })
})
