// Store-backed implementation of the DarkPoolClient interface. The trivial
// MockClient in ../client.ts unblocked the UI shell; this one is what the
// trading panels actually wire against. All five unary RPCs and the
// auction stream read from the Zustand mock-store at lib/mock-store.ts.
//
// Hand it to `createDarkPoolClient({ mockClient: new StoreMockClient(...) })`
// from F0.4's factory, or call it directly in panel-level tests.

import { create as createMessage } from '@bufbuild/protobuf'

import { mockStore as defaultMockStore } from '../../mock-store'
import type { MockStore } from '../../mock-store'
import type { StoreApi } from 'zustand/vanilla'

import { DEFAULT_PAIR } from './factories'

import {
  DARK_POOL_ERROR_CODES,
  DarkPoolError,
  type DarkPoolClient,
  type StreamOptions,
} from '../client'

import {
  AuctionEventSchema,
  CancelOrderResponseSchema,
  GetAuctionHistoryResponseSchema,
  GetOrderResponseSchema,
  PlaceOrderResponseSchema,
} from '../proto/darkpool/v1/darkpool_pb.js'
import type {
  AuctionEvent,
  AuctionSummary,
  CancelOrderRequest,
  CancelOrderResponse,
  GetAuctionHistoryRequest,
  GetAuctionHistoryResponse,
  GetOrderRequest,
  GetOrderResponse,
  PlaceOrderRequest,
  PlaceOrderResponse,
  StreamAuctionsRequest,
} from '../proto/darkpool/v1/darkpool_pb.js'
import { Side } from '../proto/darkpool/v1/darkpool_pb.js'
// Order book is mock-only since #178 — hand-written types, not generated.
import type { OrderBook, OrderBookRequest } from '../orderbook.js'

// ─── PlaceOrder payload bridge ────────────────────────────────────────────
//
// The wire's PlaceOrderRequest only carries the commitment + proof +
// encrypted_payload trio (the engine decrypts server-side). For the mock
// surface we need a price / size / side to actually drop the order into
// the book, so consumers pass a `mockPayload` alongside the wire request.
//
// In Phase 2 the browser ECIES + WASM prover (#96, #97, #98) populate the
// real commitment/proof/payload, and `placeOrder` calls flow to the REST
// client instead — at which point this bridge falls out of the hot path.

export interface MockOrderPayload {
  side: Side
  price: string
  size: string
}

const MOCK_PAYLOAD_KEY = Symbol.for('darkpool.mocks.placeOrderPayload')

type PlaceOrderRequestWithPayload = PlaceOrderRequest & {
  [MOCK_PAYLOAD_KEY]?: MockOrderPayload
}

/**
 * Attach mock order details (side/price/size) to a `PlaceOrderRequest`
 * so the store-backed client can drop it into the book. The annotation
 * is keyed by a `Symbol.for(...)` slot so it never leaks into the proto
 * serialization (`toJson` ignores symbols) — Phase 2's real placeOrder
 * call is byte-identical regardless of whether this key was set.
 */
export function withMockPayload(
  req: PlaceOrderRequest,
  payload: MockOrderPayload
): PlaceOrderRequest {
  ;(req as PlaceOrderRequestWithPayload)[MOCK_PAYLOAD_KEY] = payload
  return req
}

function readMockPayload(req: PlaceOrderRequest): MockOrderPayload | undefined {
  return (req as PlaceOrderRequestWithPayload)[MOCK_PAYLOAD_KEY]
}

// ─── StoreMockClient ──────────────────────────────────────────────────────

export interface StoreMockClientOptions {
  /** Defaults to the HMR-singleton `mockStore`. */
  store?: StoreApi<MockStore>
  /** Defaults to ETH/USDC; overrides the response.pair field. */
  pair?: string
}

export class StoreMockClient implements DarkPoolClient {
  private readonly store: StoreApi<MockStore>
  private readonly pair: string

  constructor(opts: StoreMockClientOptions = {}) {
    this.store = opts.store ?? defaultMockStore
    this.pair = opts.pair ?? DEFAULT_PAIR
  }

  async placeOrder(req: PlaceOrderRequest): Promise<PlaceOrderResponse> {
    const payload = readMockPayload(req)
    if (!payload) {
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.INVALID_ARGUMENT,
        'StoreMockClient.placeOrder requires mock payload metadata; ' +
          'wrap the request with `withMockPayload(req, { side, price, size })`.'
      )
    }
    if (payload.side === Side.UNSPECIFIED) {
      throw new DarkPoolError(DARK_POOL_ERROR_CODES.INVALID_ARGUMENT, 'side must be BUY or SELL')
    }
    const order = this.store.getState().placeOrder({
      side: payload.side,
      price: payload.price,
      size: payload.size,
      commitmentKey: bytesPreview(req.commitment),
    })
    return createMessage(PlaceOrderResponseSchema, { order })
  }

  async cancelOrder(req: CancelOrderRequest): Promise<CancelOrderResponse> {
    const ok = this.store.getState().cancelOrder(req.orderId)
    if (!ok) {
      throw new DarkPoolError(DARK_POOL_ERROR_CODES.NOT_FOUND, `Order ${req.orderId} not found`)
    }
    return createMessage(CancelOrderResponseSchema, {})
  }

  async getOrder(req: GetOrderRequest): Promise<GetOrderResponse> {
    const order = this.store.getState().openOrders.find((o) => o.id === req.orderId)
    if (!order) {
      throw new DarkPoolError(DARK_POOL_ERROR_CODES.NOT_FOUND, `Order ${req.orderId} not found`)
    }
    return createMessage(GetOrderResponseSchema, { order })
  }

  async getOrderBook(req: OrderBookRequest): Promise<OrderBook> {
    const book = this.store.getState().orderbook
    // Echo the requested pair when the client asked for one. Empty `pair` →
    // fall back to the store pair.
    return {
      pair: req.pair || book.pair || this.pair,
      bids: book.bids,
      asks: book.asks,
    }
  }

  async getAuctionHistory(req: GetAuctionHistoryRequest): Promise<GetAuctionHistoryResponse> {
    const all = this.store.getState().recentAuctions
    const limit = req.limit > 0 ? req.limit : all.length
    return createMessage(GetAuctionHistoryResponseSchema, {
      auctions: all.slice(0, limit),
    })
  }

  async *streamAuctions(
    req: StreamAuctionsRequest,
    opts?: StreamOptions
  ): AsyncIterable<AuctionEvent> {
    const signal = opts?.signal
    const pairFilter = req.pair
    const queue: AuctionSummary[] = []
    let waker: (() => void) | null = null
    const wake = () => {
      const w = waker
      waker = null
      if (w) w()
    }

    const unsubscribe = this.store.subscribe((state, prev) => {
      const next = state.recentAuctions
      const prior = prev.recentAuctions
      // Push only entries that landed since the last notification, in
      // chronological order, so consumers see the same event sequence
      // they would over the real server-side stream.
      const newOnes: AuctionSummary[] = []
      for (const auction of next) {
        if (prior.includes(auction)) break
        newOnes.unshift(auction)
      }
      for (const a of newOnes) {
        if (pairFilter && a.pair !== pairFilter) continue
        queue.push(a)
      }
      if (newOnes.length > 0) wake()
    })

    const onAbort = () => wake()
    signal?.addEventListener('abort', onAbort, { once: true })

    try {
      if (signal?.aborted) return
      while (!signal?.aborted) {
        while (queue.length > 0) {
          const a = queue.shift()!
          yield createMessage(AuctionEventSchema, {
            auctionId: a.auctionId,
            pair: a.pair,
            clearingPrice: a.clearingPrice,
            matchedVolume: a.matchedVolume,
            matchCount: a.matchCount,
            timestampUnix: a.timestampUnix,
          })
        }
        if (signal?.aborted) return
        await new Promise<void>((resolve) => {
          waker = resolve
        })
      }
    } finally {
      unsubscribe()
      signal?.removeEventListener('abort', onAbort)
    }
  }
}

function bytesPreview(b: Uint8Array): string {
  if (b.length === 0) return ''
  const slice = Array.from(b.slice(0, 4))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
  return `mock-commitment-${slice}`
}
