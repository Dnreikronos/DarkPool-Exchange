'use client'

import { useMemo, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

import { DEFAULT_PAIR } from '../../../lib/sdk/mocks/factories'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'

import { computeDepthRows, formatDelta } from './depth'
import { DepthTable, type DepthTableSide } from './DepthTable'
import { OrderBookHeader } from './OrderBookHeader'
import { SpreadRow } from './SpreadRow'
import { OrderBookEmpty, OrderBookError, OrderBookLoading } from './states'
import { useOrderBook, useRecentAuctions } from './useOrderBook'

export interface OrderBookProps {
  pair?: string
  /**
   * Fires when the user clicks a price level. F1.9 (#76) wires this to the
   * order-entry form's price input; absent that handler the row still gets
   * its hover/focus affordances but is otherwise a no-op.
   */
  onPriceSelect?: (price: string, side: Side.BUY | Side.SELL) => void
  /** Override polling cadence for Storybook / tests. */
  refetchIntervalMs?: number
}

/**
 * Root orderbook component.
 *
 * Ships its own `QueryClientProvider` because Shell.tsx + the trading-app
 * layout are off-limits in F1.6's scope and a wave-4 follow-up has not yet
 * hoisted a shared client. A consumer that *does* already wrap children
 * with a `QueryClientProvider` should render `OrderBookContent` directly
 * — it sees the ancestor's client and uses the shared cache.
 */
export function OrderBook({ pair, onPriceSelect, refetchIntervalMs }: OrderBookProps) {
  return (
    <QueryClientProvider client={getScopedClient()}>
      <OrderBookContent
        pair={pair}
        onPriceSelect={onPriceSelect}
        refetchIntervalMs={refetchIntervalMs}
      />
    </QueryClientProvider>
  )
}

/**
 * The contents, sans QueryClientProvider — usable directly when an
 * ancestor already provides one (Wave-4 cleanup will hoist a shared
 * client into the trading layout).
 */
export function OrderBookContent({
  pair,
  onPriceSelect,
  refetchIntervalMs,
}: OrderBookProps): JSX.Element {
  const effectivePair = pair ?? DEFAULT_PAIR
  const book = useOrderBook({ pair: effectivePair, refetchIntervalMs })
  const auctions = useRecentAuctions({ pair: effectivePair, limit: 2, refetchIntervalMs })

  const depth = useMemo(() => {
    if (!book.data) return null
    return computeDepthRows(book.data.bids, book.data.asks)
  }, [book.data])

  const header = useMemo(() => {
    const last = auctions.data?.auctions[0]?.clearingPrice ?? null
    const prev = auctions.data?.auctions[1]?.clearingPrice ?? null
    return {
      clearingPrice: last,
      delta: last !== null ? formatDelta(last, prev) : null,
    }
  }, [auctions.data])

  const handleSelect = onPriceSelect
    ? (price: string, side: DepthTableSide) => onPriceSelect(price, side)
    : undefined

  let body: ReactNode
  if (book.isLoading && !book.data) {
    body = <OrderBookLoading />
  } else if (book.isError) {
    body = (
      <OrderBookError
        message={book.error instanceof Error ? book.error.message : undefined}
        onRetry={() => void book.refetch()}
      />
    )
  } else if (depth === null || (depth.bids.length === 0 && depth.asks.length === 0)) {
    body = <OrderBookEmpty />
  } else {
    const bestBid = depth.bids[0]?.level.price ?? null
    const bestAsk = depth.asks[0]?.level.price ?? null
    body = (
      <div className="flex flex-col">
        <ColumnHeader />
        <DepthTable rows={depth.asks} side={Side.SELL} reverse onSelect={handleSelect} />
        <SpreadRow bestBid={bestBid} bestAsk={bestAsk} />
        <DepthTable rows={depth.bids} side={Side.BUY} onSelect={handleSelect} />
      </div>
    )
  }

  return (
    <div className="flex h-full w-full flex-col" data-testid="orderbook">
      <OrderBookHeader clearingPrice={header.clearingPrice} delta={header.delta} />
      <div className="flex flex-1 flex-col overflow-y-auto">{body}</div>
    </div>
  )
}

function ColumnHeader() {
  return (
    <div
      role="row"
      className="grid grid-cols-3 gap-2 border-b border-brand-border px-4 py-2 font-mono text-label-md uppercase text-brand-muted"
    >
      <span className="text-left">Price</span>
      <span className="text-right">Size</span>
      <span className="text-right">Total</span>
    </div>
  )
}

/**
 * Lazily-built QueryClient at module scope so HMR reloads in dev don't
 * reset the in-flight orderbook query on every save. A follow-up that
 * hoists a shared client to the trading layout can use
 * `OrderBookContent` directly and skip this scoped instance.
 */
let scopedClient: QueryClient | null = null
function getScopedClient(): QueryClient {
  if (scopedClient) return scopedClient
  scopedClient = new QueryClient({
    defaultOptions: {
      queries: {
        // Polling-driven; we never want to retry on a transient hiccup mid
        // tick because the next tick is 1 s away.
        retry: false,
        refetchOnWindowFocus: false,
      },
    },
  })
  return scopedClient
}
