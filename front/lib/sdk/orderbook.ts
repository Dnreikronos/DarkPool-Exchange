// Order book domain types — mock-only as of #178.
//
// The backend removed the public pre-settlement order book in
// commit d4cb56b ("Remove pre-settlement orderbook; scope order reads to
// caller"): there is no `GetOrderBook` RPC and no `GetOrderBookResponse` /
// `PriceLevel` message in crates/dp-api/proto/darkpool/v1/darkpool.proto
// anymore. Serving live cross-trader depth would defeat the dark pool — an
// observer could reconstruct incoming liquidity and cross it in the same
// auction round.
//
// The trading UI still renders an order book, but it is sourced entirely
// from the Phase-1 mock store (gate it with NEXT_PUBLIC_USE_MOCKS_ORDERBOOK).
// These hand-written types replace the formerly-generated proto messages so
// the panel, depth lib, and depth chart keep their shape without a wire
// backing. They never cross the network, so they are plain interfaces with
// no protobuf runtime — decimal fields stay canonical wire strings.

/** One aggregated price level in the (mock) order book. */
export interface PriceLevel {
  /** Canonical decimal string — never coerce to JS number. */
  price: string
  /** Aggregate resting size at this level, as a canonical decimal string. */
  totalSize: string
  /** Number of resting orders aggregated into this level. */
  orderCount: number
}

/** A full order-book snapshot: best-of-book at index 0 on each side. */
export interface OrderBook {
  pair: string
  /** Descending price (best bid first). */
  bids: PriceLevel[]
  /** Ascending price (best ask first). */
  asks: PriceLevel[]
}

/** Query for {@link DarkPoolClient.getOrderBook}. */
export interface OrderBookRequest {
  pair: string
}
