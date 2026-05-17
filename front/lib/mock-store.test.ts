import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { Decimal, fromWireSize } from './units'

import { Side } from './sdk/proto/darkpool/v1/darkpool_pb.js'

import { DEFAULT_AUCTION_HISTORY, RECENT_AUCTIONS_CAP, createMockStore } from './mock-store'

const FROZEN_NOW = 1700000000
const SEED = 42

function freshStore(seed = SEED) {
  return createMockStore({ seed, now: () => FROZEN_NOW, mid: '3000', depth: 8 })
}

describe('createMockStore — initial state', () => {
  it('hydrates orderbook, balances, and seed recent auctions', () => {
    const store = freshStore()
    const s = store.getState()
    expect(s.pair).toBe('ETH/USDC')
    expect(s.orderbook.bids.length).toBeGreaterThan(0)
    expect(s.orderbook.asks.length).toBeGreaterThan(0)
    expect(s.balances.weth).toBe('10')
    expect(s.balances.usdc).toBe('25000')
    expect(s.openOrders).toEqual([])
    expect(s.fillHistory).toEqual([])
    expect(s.recentAuctions).toHaveLength(DEFAULT_AUCTION_HISTORY)
  })

  it('two stores with the same seed produce equal initial orderbooks', () => {
    const a = freshStore().getState().orderbook
    const b = freshStore().getState().orderbook
    expect(a.bids.map((l) => [l.price, l.totalSize])).toEqual(
      b.bids.map((l) => [l.price, l.totalSize])
    )
    expect(a.asks.map((l) => [l.price, l.totalSize])).toEqual(
      b.asks.map((l) => [l.price, l.totalSize])
    )
  })
})

describe('placeOrder', () => {
  it('prepends to openOrders and drops the size into the matching level', () => {
    const store = freshStore()
    const beforeBid = store.getState().orderbook.bids.find((l) => l.price === '2995')
    const beforeSize = beforeBid ? new Decimal(beforeBid.totalSize) : new Decimal(0)
    const order = store.getState().placeOrder({
      side: Side.BUY,
      price: '2995',
      size: '1.5',
    })
    const s = store.getState()
    expect(s.openOrders[0].id).toBe(order.id)
    expect(s.openOrders[0].remainingSize).toBe('1.5')
    const level = s.orderbook.bids.find((l) => l.price === '2995')
    expect(level).toBeDefined()
    expect(new Decimal(level!.totalSize).equals(beforeSize.plus('1.5'))).toBe(true)
  })

  it('inserts a brand-new level when the price is off-grid, keeping bids sorted desc', () => {
    const store = freshStore()
    store.getState().placeOrder({ side: Side.BUY, price: '2950.50', size: '0.25' })
    const bids = store.getState().orderbook.bids
    for (let i = 1; i < bids.length; i++) {
      expect(new Decimal(bids[i].price).lt(bids[i - 1].price)).toBe(true)
    }
    expect(bids.some((l) => l.price === '2950.5')).toBe(true)
  })

  it('rejects sizes above the wire precision via the units helper', () => {
    const store = freshStore()
    expect(() =>
      store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '0.000000001' })
    ).toThrow(/8dp/)
  })
})

describe('cancelOrder', () => {
  it('removes from openOrders and refunds the level', () => {
    const store = freshStore()
    const order = store.getState().placeOrder({
      side: Side.SELL,
      price: '3005',
      size: '1.5',
    })
    const before = store.getState().orderbook.asks.find((l) => l.price === '3005')!
    const ok = store.getState().cancelOrder(order.id)
    expect(ok).toBe(true)
    const after = store.getState().orderbook.asks.find((l) => l.price === '3005')
    if (before.orderCount > 1) {
      expect(after).toBeDefined()
      expect(new Decimal(after!.totalSize).equals(new Decimal(before.totalSize).minus('1.5'))).toBe(
        true
      )
    } else {
      expect(after).toBeUndefined()
    }
    expect(store.getState().openOrders.some((o) => o.id === order.id)).toBe(false)
  })

  it('returns false for unknown ids without touching state', () => {
    const store = freshStore()
    const before = store.getState()
    const ok = store.getState().cancelOrder('ghost')
    expect(ok).toBe(false)
    expect(store.getState()).toBe(before)
  })
})

describe('perturbOrderbook', () => {
  it('rewrites level sizes while keeping price grid stable', () => {
    const store = freshStore()
    const before = store.getState().orderbook
    store.getState().perturbOrderbook()
    const after = store.getState().orderbook
    expect(after.bids.map((l) => l.price)).toEqual(before.bids.map((l) => l.price))
    expect(after.asks.map((l) => l.price)).toEqual(before.asks.map((l) => l.price))
    const changed = after.bids.some((l, i) => l.totalSize !== before.bids[i].totalSize)
    expect(changed).toBe(true)
  })
})

describe('runAuction', () => {
  it('prepends a fresh AuctionSummary with a clearing price near the current mid', () => {
    const store = freshStore()
    const beforeCount = store.getState().recentAuctions.length
    const auction = store.getState().runAuction()
    expect(store.getState().recentAuctions[0].auctionId).toBe(auction.auctionId)
    expect(store.getState().recentAuctions).toHaveLength(beforeCount + 1)
    const cp = new Decimal(auction.clearingPrice)
    expect(cp.gte('2900')).toBe(true)
    expect(cp.lte('3100')).toBe(true)
    expect(auction.timestampUnix).toBe(BigInt(FROZEN_NOW))
  })

  it('caps recentAuctions at RECENT_AUCTIONS_CAP', () => {
    const store = freshStore()
    for (let i = 0; i < RECENT_AUCTIONS_CAP + 5; i++) store.getState().runAuction()
    expect(store.getState().recentAuctions).toHaveLength(RECENT_AUCTIONS_CAP)
  })

  it('consumes an in-the-money open order into fillHistory', () => {
    const store = freshStore()
    // A buy at far above mid is guaranteed to clear in the next auction.
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '10000', size: '0.5' })
    store.getState().runAuction()
    const s = store.getState()
    expect(s.openOrders.some((o) => o.id === placed.id)).toBe(false)
    expect(s.fillHistory.length).toBeGreaterThan(0)
    expect(s.fillHistory[0].orderId).toBe(placed.id)
    expect(fromWireSize(s.fillHistory[0].size).equals(new Decimal('0.5'))).toBe(true)
  })

  it('leaves the book and orders alone when nothing matches', () => {
    const store = freshStore()
    // A sell well above any plausible clearing won't clear.
    const placed = store.getState().placeOrder({ side: Side.SELL, price: '100000', size: '0.1' })
    store.getState().runAuction()
    expect(store.getState().openOrders.some((o) => o.id === placed.id)).toBe(true)
    expect(store.getState().fillHistory).toEqual([])
  })

  it('refunds the consumed order back out of the orderbook level', () => {
    const store = freshStore()
    // Pick an off-grid price so the order owns its level entirely.
    const placed = store.getState().placeOrder({
      side: Side.BUY,
      price: '9999.50',
      size: '0.75',
    })
    const levelBefore = store.getState().orderbook.bids.find((l) => l.price === '9999.5')
    expect(levelBefore).toBeDefined()
    expect(levelBefore!.totalSize).toBe('0.75')
    store.getState().runAuction()
    const s = store.getState()
    expect(s.openOrders.some((o) => o.id === placed.id)).toBe(false)
    // Level had a single order; refund drains it entirely.
    expect(s.orderbook.bids.find((l) => l.price === '9999.5')).toBeUndefined()
  })

  it('only refunds the filled order, not other orders at the same price', () => {
    const store = freshStore()
    const a = store.getState().placeOrder({ side: Side.BUY, price: '9998', size: '0.4' })
    const b = store.getState().placeOrder({ side: Side.BUY, price: '9998', size: '0.6' })
    const levelBefore = store.getState().orderbook.bids.find((l) => l.price === '9998')!
    expect(levelBefore.orderCount).toBeGreaterThanOrEqual(2)
    const totalBefore = new Decimal(levelBefore.totalSize)
    store.getState().runAuction()
    const s = store.getState()
    // Exactly one of a/b was consumed.
    const remainingIds = s.openOrders.map((o) => o.id)
    const consumed = remainingIds.includes(a.id) ? b : a
    const surviving = consumed === a ? b : a
    expect(remainingIds).toContain(surviving.id)
    expect(remainingIds).not.toContain(consumed.id)
    const levelAfter = s.orderbook.bids.find((l) => l.price === '9998')!
    expect(levelAfter).toBeDefined()
    expect(levelAfter.orderCount).toBe(levelBefore.orderCount - 1)
    expect(
      new Decimal(levelAfter.totalSize).equals(totalBefore.minus(consumed.remainingSize))
    ).toBe(true)
  })
})

describe('start / stop tick loop', () => {
  beforeEach(() => {
    vi.useFakeTimers({ now: FROZEN_NOW * 1000 })
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('fires perturb on perturbMs and runAuction on auctionMs', () => {
    const store = freshStore()
    const beforeBook = store.getState().orderbook
    const beforeAuctions = store.getState().recentAuctions.length
    store.getState().start({ perturbMs: 100, auctionMs: 500 })

    vi.advanceTimersByTime(100)
    expect(store.getState().orderbook).not.toBe(beforeBook)

    vi.advanceTimersByTime(400)
    expect(store.getState().recentAuctions.length).toBe(beforeAuctions + 1)

    store.getState().stop()
  })

  it('start is idempotent — calling twice does not double the timer count', () => {
    const store = freshStore()
    store.getState().start({ perturbMs: 100, auctionMs: 500 })
    store.getState().start({ perturbMs: 100, auctionMs: 500 })
    const beforeAuctions = store.getState().recentAuctions.length
    vi.advanceTimersByTime(500)
    expect(store.getState().recentAuctions.length).toBe(beforeAuctions + 1)
    store.getState().stop()
  })

  it('stop halts further ticks', () => {
    const store = freshStore()
    store.getState().start({ perturbMs: 100, auctionMs: 500 })
    store.getState().stop()
    const snapshot = store.getState().orderbook
    vi.advanceTimersByTime(2000)
    expect(store.getState().orderbook).toBe(snapshot)
  })
})

describe('seed / reset', () => {
  it('seed re-initialises with a fresh faker handle', () => {
    const store = freshStore()
    store.getState().placeOrder({ side: Side.BUY, price: '2999', size: '0.5' })
    expect(store.getState().openOrders).toHaveLength(1)
    store.getState().seed({ seed: 7, mid: '3000', depth: 4 })
    const s = store.getState()
    expect(s.openOrders).toEqual([])
    expect(s.fillHistory).toEqual([])
    expect(s.orderbook.bids).toHaveLength(4)
    expect(s.orderbook.asks).toHaveLength(4)
  })

  it('reset wipes runtime state but keeps the original seed', () => {
    const a = freshStore()
    const baselineBidPrices = a.getState().orderbook.bids.map((l) => l.price)
    a.getState().placeOrder({ side: Side.BUY, price: '2999', size: '0.5' })
    a.getState().reset()
    expect(a.getState().openOrders).toEqual([])
    // After reset, the orderbook is regenerated from the same seeded ctx,
    // so the price grid still has the same set of levels.
    expect(a.getState().orderbook.bids.map((l) => l.price)).toEqual(baselineBidPrices)
  })

  it('seed stops a running tick loop', () => {
    vi.useFakeTimers({ now: FROZEN_NOW * 1000 })
    const store = freshStore()
    store.getState().start({ perturbMs: 100, auctionMs: 500 })
    store.getState().seed({ seed: 3 })
    const snapshot = store.getState().orderbook
    vi.advanceTimersByTime(2000)
    expect(store.getState().orderbook).toBe(snapshot)
    vi.useRealTimers()
  })
})
