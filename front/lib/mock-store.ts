// Simulated backend for Phase 1 of the trading app. A Zustand store holds
// the orderbook, recent auctions, balances, open orders, and fill history;
// a tick loop perturbs the book every second and runs a new clearing
// auction every five. Panels (#71/#73/#74) read from the store via the
// React hook, and `StoreMockClient` in lib/sdk/mocks/client.ts adapts the
// same state to the `DarkPoolClient` interface so the entire UI flips to
// the REST surface in Phase 2 with no callsite changes.
//
// `createMockStore` is the testable factory; `mockStore` is the runtime
// singleton, stashed on a `Symbol.for`-keyed slot of `globalThis` so it
// survives Next.js hot-module reloads in dev.

import { create as createMessage } from '@bufbuild/protobuf'
import { useSyncExternalStore } from 'react'
import { createStore, type StoreApi } from 'zustand/vanilla'

import { Decimal, toWireSize } from './units'

import {
  GetOrderBookResponseSchema,
  PriceLevelSchema,
  Side,
} from './sdk/proto/darkpool/v1/darkpool_pb.js'
import type {
  AuctionSummary,
  GetOrderBookResponse,
  OrderInfo,
  PriceLevel,
} from './sdk/proto/darkpool/v1/darkpool_pb.js'

import {
  DEFAULT_MID,
  type Balances,
  type FactoryContext,
  type Fill,
  createFactoryContext,
  midFromBook,
  mockAuctionSummary,
  mockBalances,
  mockFill,
  mockOrderBook,
  mockOrderInfo,
  scaleWireSize,
} from './sdk/mocks/factories'

// ─── Public state shape ───────────────────────────────────────────────────

export interface MockStoreState {
  pair: string
  orderbook: GetOrderBookResponse
  /** Newest first. Capped at `RECENT_AUCTIONS_CAP`. */
  recentAuctions: AuctionSummary[]
  balances: Balances
  /** Orders the user placed but the engine hasn't cleared yet. Newest first. */
  openOrders: OrderInfo[]
  /** Append-only history of partial/full fills. Newest first. */
  fillHistory: Fill[]
}

export interface PlaceOrderInput {
  side: Side
  price: Decimal | string
  size: Decimal | string
  commitmentKey?: string
}

export interface TickOptions {
  perturbMs?: number
  auctionMs?: number
}

export interface SeedOptions {
  seed?: number
  pair?: string
  mid?: Decimal | string
  /** Number of opening recent auctions surfaced to the tape on init. */
  auctionHistory?: number
  /** Number of price levels per side at seed time. */
  depth?: number
}

export interface MockStoreActions {
  /** Trader-facing: push a new order. Mutates openOrders + orderbook. */
  placeOrder(input: PlaceOrderInput): OrderInfo
  /** Trader-facing: remove an order and drain its level from the book. */
  cancelOrder(orderId: string): boolean
  /** 1 s tick: nudge level sizes so the book looks alive. */
  perturbOrderbook(): void
  /** 5 s tick: produce a new clearing auction sampled around current mid. */
  runAuction(): AuctionSummary
  /** Start the perturb/auction timers. No-op if already running. */
  start(opts?: TickOptions): void
  /** Cancel both timers. Safe to call before start(). */
  stop(): void
  /** Re-initialise with a fresh faker seed. Stops the loop first. */
  seed(opts?: SeedOptions): void
  /** Re-initialise with the same factory ctx. Stops the loop first. */
  reset(): void
}

export type MockStore = MockStoreState & MockStoreActions

// ─── Tunables ─────────────────────────────────────────────────────────────

export const RECENT_AUCTIONS_CAP = 200
export const FILL_HISTORY_CAP = 500
export const DEFAULT_DEPTH = 12
export const DEFAULT_AUCTION_HISTORY = 6
const PERTURB_PROBABILITY = 0.5
const UP_FACTOR = '1.05'
const DOWN_FACTOR = '0.95'
const SIZE_FLOOR = '0.0001'

// ─── Factory ──────────────────────────────────────────────────────────────

export interface CreateMockStoreOptions extends SeedOptions {
  /** Injectable clock; defaults to wall time. */
  now?: () => number
}

export function createMockStore(opts: CreateMockStoreOptions = {}): StoreApi<MockStore> {
  let ctx = createFactoryContext({ seed: opts.seed, pair: opts.pair, now: opts.now })
  let perturbTimer: ReturnType<typeof setInterval> | null = null
  let auctionTimer: ReturnType<typeof setInterval> | null = null

  const initialState = (current: FactoryContext, init: SeedOptions): MockStoreState => {
    const mid = init.mid !== undefined ? new Decimal(init.mid.toString()) : DEFAULT_MID
    const depth = init.depth ?? DEFAULT_DEPTH
    const historyCount = init.auctionHistory ?? DEFAULT_AUCTION_HISTORY
    return {
      pair: current.pair,
      orderbook: mockOrderBook(current, { pair: current.pair, mid, depth }),
      recentAuctions: Array.from({ length: historyCount }, () =>
        mockAuctionSummary(current, { mid, pair: current.pair })
      ).sort((a, b) => Number(b.timestampUnix - a.timestampUnix)),
      balances: mockBalances(current),
      openOrders: [],
      fillHistory: [],
    }
  }

  const store = createStore<MockStore>((set, get) => ({
    ...initialState(ctx, opts),

    placeOrder(input) {
      const order = mockOrderInfo(ctx, {
        side: input.side,
        price: input.price,
        size: input.size,
        commitmentKey: input.commitmentKey,
      })
      set((s) => ({
        openOrders: [order, ...s.openOrders],
        orderbook: addOrderToBook(s.orderbook, order),
      }))
      return order
    },

    cancelOrder(orderId) {
      const order = get().openOrders.find((o) => o.id === orderId)
      if (!order) return false
      set((s) => ({
        openOrders: s.openOrders.filter((o) => o.id !== orderId),
        orderbook: removeOrderFromBook(s.orderbook, order),
      }))
      return true
    },

    perturbOrderbook() {
      set((s) => ({ orderbook: perturbBook(ctx, s.orderbook) }))
    },

    runAuction() {
      const book = get().orderbook
      const mid = book.bids.length > 0 && book.asks.length > 0 ? midFromBook(book) : DEFAULT_MID
      const auction = mockAuctionSummary(ctx, {
        mid,
        pair: ctx.pair,
        timestampUnix: BigInt(ctx.now()),
      })
      const fill = consumeOpenOrder(ctx, get().openOrders, auction)
      set((s) => ({
        recentAuctions: [auction, ...s.recentAuctions].slice(0, RECENT_AUCTIONS_CAP),
        openOrders: fill ? s.openOrders.filter((o) => o.id !== fill.orderId) : s.openOrders,
        fillHistory: fill ? [fill, ...s.fillHistory].slice(0, FILL_HISTORY_CAP) : s.fillHistory,
      }))
      return auction
    },

    start(tickOpts) {
      const perturbMs = tickOpts?.perturbMs ?? 1000
      const auctionMs = tickOpts?.auctionMs ?? 5000
      if (perturbTimer !== null || auctionTimer !== null) return
      perturbTimer = setInterval(() => get().perturbOrderbook(), perturbMs)
      auctionTimer = setInterval(() => get().runAuction(), auctionMs)
    },

    stop() {
      if (perturbTimer !== null) {
        clearInterval(perturbTimer)
        perturbTimer = null
      }
      if (auctionTimer !== null) {
        clearInterval(auctionTimer)
        auctionTimer = null
      }
    },

    seed(seedOpts) {
      get().stop()
      ctx = createFactoryContext({
        seed: seedOpts?.seed,
        pair: seedOpts?.pair ?? ctx.pair,
        now: opts.now,
      })
      set(initialState(ctx, seedOpts ?? {}))
    },

    reset() {
      get().stop()
      set(initialState(ctx, opts))
    },
  }))

  return store
}

// ─── HMR-safe singleton ───────────────────────────────────────────────────

const MOCK_STORE_KEY = Symbol.for('darkpool.mockStore.v1')

type GlobalSlot = Record<symbol, unknown>
const globalSlot = globalThis as unknown as GlobalSlot

const existing = globalSlot[MOCK_STORE_KEY] as StoreApi<MockStore> | undefined
export const mockStore: StoreApi<MockStore> = existing ?? createMockStore({ seed: 1 })
globalSlot[MOCK_STORE_KEY] = mockStore

// ─── React binding ────────────────────────────────────────────────────────
//
// Avoids pulling `useStore` from `zustand` so this file doesn't import the
// React-specific entrypoint (the vanilla store is the one we treat as the
// API surface). The `useSyncExternalStore` adapter is the same one
// `useStore` builds under the hood.

export function useMockStore<T>(selector: (state: MockStore) => T): T {
  return useSyncExternalStore(
    mockStore.subscribe,
    () => selector(mockStore.getState()),
    () => selector(mockStore.getState())
  )
}

// ─── Orderbook mutation helpers ──────────────────────────────────────────

function perturbBook(ctx: FactoryContext, book: GetOrderBookResponse): GetOrderBookResponse {
  const tweak = (level: PriceLevel): PriceLevel => {
    if (!ctx.faker.datatype.boolean({ probability: PERTURB_PROBABILITY })) return level
    const goesUp = ctx.faker.datatype.boolean()
    const factor = goesUp ? UP_FACTOR : DOWN_FACTOR
    return createMessage(PriceLevelSchema, {
      price: level.price,
      totalSize: scaleWireSize(level.totalSize, factor, SIZE_FLOOR),
      orderCount: level.orderCount,
    })
  }
  return createMessage(GetOrderBookResponseSchema, {
    pair: book.pair,
    bids: book.bids.map(tweak),
    asks: book.asks.map(tweak),
  })
}

function addOrderToBook(book: GetOrderBookResponse, order: OrderInfo): GetOrderBookResponse {
  const isBuy = order.side === Side.BUY
  const sideLevels = isBuy ? book.bids : book.asks
  const otherLevels = isBuy ? book.asks : book.bids
  const idx = sideLevels.findIndex((l) => l.price === order.price)

  let updated: PriceLevel[]
  if (idx >= 0) {
    const existing = sideLevels[idx]
    updated = [...sideLevels]
    updated[idx] = createMessage(PriceLevelSchema, {
      price: existing.price,
      totalSize: toWireSize(new Decimal(existing.totalSize).plus(order.remainingSize)),
      orderCount: existing.orderCount + 1,
    })
  } else {
    const inserted = createMessage(PriceLevelSchema, {
      price: order.price,
      totalSize: order.remainingSize,
      orderCount: 1,
    })
    updated = sortSide(isBuy, [...sideLevels, inserted])
  }
  return createMessage(GetOrderBookResponseSchema, {
    pair: book.pair,
    bids: isBuy ? updated : otherLevels,
    asks: isBuy ? otherLevels : updated,
  })
}

function removeOrderFromBook(book: GetOrderBookResponse, order: OrderInfo): GetOrderBookResponse {
  const isBuy = order.side === Side.BUY
  const sideLevels = isBuy ? book.bids : book.asks
  const otherLevels = isBuy ? book.asks : book.bids
  const idx = sideLevels.findIndex((l) => l.price === order.price)
  if (idx < 0) return book

  const existing = sideLevels[idx]
  const newSize = new Decimal(existing.totalSize).minus(order.remainingSize)
  let updated: PriceLevel[]
  if (newSize.lte(0) || existing.orderCount <= 1) {
    updated = sideLevels.filter((_, i) => i !== idx)
  } else {
    updated = [...sideLevels]
    updated[idx] = createMessage(PriceLevelSchema, {
      price: existing.price,
      totalSize: toWireSize(newSize),
      orderCount: existing.orderCount - 1,
    })
  }
  return createMessage(GetOrderBookResponseSchema, {
    pair: book.pair,
    bids: isBuy ? updated : otherLevels,
    asks: isBuy ? otherLevels : updated,
  })
}

function sortSide(isBuy: boolean, levels: PriceLevel[]): PriceLevel[] {
  const sorted = [...levels]
  sorted.sort((a, b) =>
    isBuy
      ? new Decimal(b.price).cmp(new Decimal(a.price))
      : new Decimal(a.price).cmp(new Decimal(b.price))
  )
  return sorted
}

// ─── Auction matcher ──────────────────────────────────────────────────────
//
// Picks one open order whose price is on the right side of the clearing
// price and turns it into a Fill. Real matching is out of scope; this is
// just enough for the open-orders panel (#77) and the portfolio P&L
// (#78) to show movement.

function consumeOpenOrder(
  ctx: FactoryContext,
  openOrders: OrderInfo[],
  auction: AuctionSummary
): Fill | null {
  const clearing = new Decimal(auction.clearingPrice)
  const eligible = openOrders.filter((o) => {
    const price = new Decimal(o.price)
    return o.side === Side.BUY ? price.gte(clearing) : price.lte(clearing)
  })
  if (eligible.length === 0) return null
  const order = eligible[0]
  return mockFill(ctx, {
    orderId: order.id,
    auctionId: auction.auctionId,
    side: order.side,
    price: auction.clearingPrice,
    size: order.remainingSize,
    timestampUnix: auction.timestampUnix,
  })
}

// ─── Test helpers ─────────────────────────────────────────────────────────

/** Reset the HMR-keyed slot. Tests only. */
export function __resetMockStoreSingletonForTests(): void {
  globalSlot[MOCK_STORE_KEY] = undefined
}

// Re-export the local Fill / Balances types for consumers that don't want
// to reach into ./sdk/mocks/factories directly.
export type { Balances, Fill } from './sdk/mocks/factories'
