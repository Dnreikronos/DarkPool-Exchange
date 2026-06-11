import './_processShim'

import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import type { DarkPoolClient } from '@/lib/sdk/client'
import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/sdk/client'
import { createFactoryContext, mockAuctionSummary, mockOrderBook } from '@/lib/sdk/mocks'
import {
  CancelOrderResponseSchema,
  GetAuctionHistoryResponseSchema,
  GetOrderResponseSchema,
  OrderInfoSchema,
  PlaceOrderResponseSchema,
  Side,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { OrderBook } from './OrderBook'

// ─── Minimal shell that mimics how Shell.tsx will frame the panel ────────

function PanelFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-[680px] w-[360px] flex-col border border-brand-border bg-brand-bg">
      <div className="flex h-9 items-center gap-3 border-b border-brand-border px-4">
        <span className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted">
          ORDERBOOK · ETH / USDC
        </span>
      </div>
      <div className="flex-1 overflow-hidden">{children}</div>
    </div>
  )
}

// ─── Client builders ─────────────────────────────────────────────────────

interface ClientOverrides {
  bookEmpty?: boolean
  noAuctions?: boolean
  raise?: 'orderbook' | 'auctions'
  seed?: number
}

function buildClient(overrides: ClientOverrides = {}): DarkPoolClient {
  const ctx = createFactoryContext({ seed: overrides.seed ?? 7 })
  const book = overrides.bookEmpty
    ? { pair: ctx.pair, bids: [], asks: [] }
    : mockOrderBook(ctx, { depth: 12 })
  const auctions = overrides.noAuctions
    ? []
    : [
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
      if (overrides.raise === 'orderbook') {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAVAILABLE, 'engine offline')
      }
      return book
    },
    async getAuctionHistory() {
      if (overrides.raise === 'auctions') {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAVAILABLE, 'engine offline')
      }
      return create(GetAuctionHistoryResponseSchema, { auctions })
    },
    async *streamAuctions() {
      // Empty stream.
    },
  }
}

function withClient(client: DarkPoolClient, Body: React.ReactNode): React.ReactElement {
  return (
    <DarkPoolClientProvider client={client}>
      <PanelFrame>{Body}</PanelFrame>
    </DarkPoolClientProvider>
  )
}

// ─── Stories ─────────────────────────────────────────────────────────────

export const Populated = () => withClient(buildClient(), <OrderBook refetchIntervalMs={2000} />)

export const PopulatedNarrow = () => (
  <DarkPoolClientProvider client={buildClient()}>
    <div className="flex h-[680px] w-[280px] flex-col border border-brand-border bg-brand-bg">
      <OrderBook refetchIntervalMs={2000} />
    </div>
  </DarkPoolClientProvider>
)

export const Empty = () =>
  withClient(buildClient({ bookEmpty: true }), <OrderBook refetchIntervalMs={2000} />)

export const NoAuctionYet = () =>
  withClient(buildClient({ noAuctions: true }), <OrderBook refetchIntervalMs={2000} />)

export const ErrorState = () =>
  withClient(buildClient({ raise: 'orderbook' }), <OrderBook refetchIntervalMs={2000} />)

export const Loading = () => {
  // A client whose getOrderBook never resolves — stays in loading forever.
  const pending: DarkPoolClient = {
    async placeOrder() {
      return create(PlaceOrderResponseSchema, { order: create(OrderInfoSchema, {}) })
    },
    async cancelOrder() {
      return create(CancelOrderResponseSchema, {})
    },
    async getOrder() {
      return create(GetOrderResponseSchema, { order: create(OrderInfoSchema, {}) })
    },
    getOrderBook() {
      return new Promise(() => {})
    },
    getAuctionHistory() {
      return new Promise(() => {})
    },
    async *streamAuctions() {},
  }
  return withClient(pending, <OrderBook refetchIntervalMs={2000} />)
}

export const WithClickToFill = () => {
  const [picked, setPicked] = React.useState<string | null>(null)
  return (
    <div className="flex flex-col gap-4">
      <DarkPoolClientProvider client={buildClient({ seed: 11 })}>
        <PanelFrame>
          <OrderBook
            refetchIntervalMs={2000}
            onPriceSelect={(price, side) =>
              setPicked(`${side === Side.BUY ? 'BID' : 'ASK'} @ ${price}`)
            }
          />
        </PanelFrame>
      </DarkPoolClientProvider>
      <div className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted">
        Last click: <span className="text-brand-fg">{picked ?? '—'}</span>
      </div>
    </div>
  )
}
