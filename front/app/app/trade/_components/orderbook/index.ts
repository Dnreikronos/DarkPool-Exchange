// Public API for the F1.6 orderbook panel.
//
// Consumers default to the `<OrderBook />` root component — it ships with
// its own QueryClientProvider and a `useDarkPoolClient()` read, so it
// works the moment it's rendered inside the trading shell (which already
// provides DarkPoolClientProvider via app/app/layout.tsx). When a
// follow-up hoists a single QueryClient up to the layout, the inner
// content (`<OrderBookContent />`) can be used directly to share the
// upstream cache.

export { OrderBook, OrderBookContent } from './OrderBook'
export type { OrderBookProps } from './OrderBook'
export {
  useOrderBook,
  useRecentAuctions,
  ORDERBOOK_POLL_MS,
} from '../../_hooks/orderbook/useOrderBook'
export type {
  UseOrderBookOptions,
  UseRecentAuctionsOptions,
} from '../../_hooks/orderbook/useOrderBook'
export { computeDepthRows, formatDelta } from '../../_lib/orderbook/depth'
export type { DepthRow, DepthRows, DeltaSign, FormattedDelta } from '../../_lib/orderbook/depth'
