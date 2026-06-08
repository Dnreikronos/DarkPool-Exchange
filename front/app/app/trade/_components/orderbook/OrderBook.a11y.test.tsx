// @vitest-environment jsdom

// Automated axe-core scan of the orderbook panel (#80). Renders against a
// stub client returning a populated book (the OrderBook.stories.tsx
// pattern) and waits for the depth tables before scanning.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { afterEach, describe, it } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'

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
import { expectNoAxeViolations } from '@/test/axe'

import { OrderBook } from './OrderBook'

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
      // Empty stream.
    },
  }
}

describe('OrderBook a11y', () => {
  afterEach(cleanup)

  it('has no axe violations with a populated depth table', async () => {
    render(
      <DarkPoolClientProvider client={buildClient()}>
        <OrderBook refetchIntervalMs={3_600_000} />
      </DarkPoolClientProvider>
    )
    await waitFor(() => {
      if (!screen.queryByText('[ BIDS ]')) throw new Error('depth table not loaded yet')
    })
    await expectNoAxeViolations()
  })
})
