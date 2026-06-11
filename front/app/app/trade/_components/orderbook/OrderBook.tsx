'use client'

import { useEffect, useMemo, useRef, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

import { useToast } from '@/components/ui/use-toast'
import { DEFAULT_PAIR } from '@/lib/sdk/mocks/factories'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { computeDepthRows, formatDelta } from '../../_lib/orderbook/depth'
import { DepthTable, type DepthTableSide } from './DepthTable'
import { OrderBookHeader } from './OrderBookHeader'
import { SpreadRow } from './SpreadRow'
import { OrderBookEmpty, OrderBookError, OrderBookLoading } from './states'
import { useOrderBook, useRecentAuctions } from '../../_hooks/orderbook/useOrderBook'

export interface OrderBookProps {
  pair?: string
  /**
   * Fires when the user clicks a price level. F1.9 (#76) wires this to the
   * order-entry form's price input; absent that handler the row still gets
   * its hover/focus affordances but is otherwise a no-op.
   */
  onPriceSelect?: (price: string, side: Side.BUY | Side.SELL) => void
  /**
   * Prices the connected trader has open orders at. F1.10 (#77) sources
   * this from the my-orders panel so the book can mark the trader's own
   * levels with a hairline edge marker on the matching row.
   */
  userPrices?: ReadonlySet<string>
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
export function OrderBook({ pair, onPriceSelect, userPrices, refetchIntervalMs }: OrderBookProps) {
  return (
    <QueryClientProvider client={getScopedClient()}>
      <OrderBookContent
        pair={pair}
        onPriceSelect={onPriceSelect}
        userPrices={userPrices}
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
  userPrices,
  refetchIntervalMs,
}: OrderBookProps): JSX.Element {
  const effectivePair = pair ?? DEFAULT_PAIR
  const book = useOrderBook({ pair: effectivePair, refetchIntervalMs })
  const auctions = useRecentAuctions({ pair: effectivePair, limit: 2, refetchIntervalMs })
  const { toast } = useToast()
  const lastErrorAtRef = useRef<unknown>(null)

  // Fire one toast on the transition into an error state. We key on the
  // error object identity so subsequent re-renders of the same failure
  // don't re-toast; a new failure (different reference) triggers again.
  // Retry lives on the inline `<OrderBookError>` body — the toast is a
  // transient ack for users looking at another panel.
  useEffect(() => {
    if (!book.isError || !book.error) {
      lastErrorAtRef.current = null
      return
    }
    if (lastErrorAtRef.current === book.error) return
    lastErrorAtRef.current = book.error
    toast({
      title: 'Orderbook unavailable',
      description: book.error instanceof Error ? book.error.message : undefined,
    })
  }, [book.isError, book.error, toast])

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
        <DepthTable
          rows={depth.asks}
          side={Side.SELL}
          reverse
          userPrices={userPrices}
          onSelect={handleSelect}
        />
        <SpreadRow bestBid={bestBid} bestAsk={bestAsk} />
        <DepthTable
          rows={depth.bids}
          side={Side.BUY}
          userPrices={userPrices}
          onSelect={handleSelect}
        />
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
  // Visual column guide, not an ARIA table — an orphan role="row" (no
  // table/rowgroup ancestor) is an axe violation (#80). The text stays
  // readable (no aria-hidden): non-clickable depth rows are bare numbers,
  // so this line is the only column context assistive tech gets.
  return (
    <div className="grid grid-cols-3 gap-2 border-b border-brand-border px-4 py-2 font-mono text-label-md uppercase text-brand-muted">
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
