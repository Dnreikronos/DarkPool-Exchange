import { describe, expect, it } from 'vitest'

import { Decimal, fromWirePrice, fromWireSize } from '../../units'

import { Side } from '../proto/darkpool/v1/darkpool_pb.js'

import {
  DEFAULT_MID,
  DEFAULT_PAIR,
  createFactoryContext,
  midFromBook,
  mockAuctionSummary,
  mockBalances,
  mockFill,
  mockOrderBook,
  mockOrderInfo,
  mockPriceLevel,
  scaleWireSize,
} from './factories'

const SEED = 42

describe('createFactoryContext', () => {
  it('returns a deterministic context when seeded', () => {
    const a = createFactoryContext({ seed: SEED })
    const b = createFactoryContext({ seed: SEED })
    expect(mockOrderInfo(a).id).toEqual(mockOrderInfo(b).id)
  })

  it('defaults pair to ETH/USDC', () => {
    const ctx = createFactoryContext({ seed: SEED })
    expect(ctx.pair).toBe(DEFAULT_PAIR)
  })

  it('uses the injected clock', () => {
    const ctx = createFactoryContext({ seed: SEED, now: () => 1700000000 })
    expect(ctx.now()).toBe(1700000000)
    expect(mockOrderInfo(ctx).submittedAtUnix).toBe(1700000000n)
  })
})

describe('mockPriceLevel', () => {
  it('emits wire-canonical price and size strings', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const level = mockPriceLevel(ctx, { price: '3000.50' })
    // price keeps requested value, normalized through toWirePrice.
    expect(level.price).toBe('3000.5')
    expect(typeof level.totalSize).toBe('string')
    expect(() => fromWireSize(level.totalSize)).not.toThrow()
    expect(level.orderCount).toBeGreaterThanOrEqual(1)
    expect(level.orderCount).toBeLessThanOrEqual(12)
  })

  it('honours an explicit totalSize override', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const level = mockPriceLevel(ctx, { price: '3000', totalSize: '2.5' })
    expect(level.totalSize).toBe('2.5')
  })

  it('refuses to coerce numeric inputs to JS number', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const level = mockPriceLevel(ctx, { price: new Decimal('0.00000001'), totalSize: '0.00000001' })
    // Round-trip through fromWire to prove no precision was lost on the way.
    expect(fromWirePrice(level.price).equals(new Decimal('0.00000001'))).toBe(true)
    expect(fromWireSize(level.totalSize).equals(new Decimal('0.00000001'))).toBe(true)
  })
})

describe('mockOrderBook', () => {
  it('returns sorted bids descending and asks ascending around the mid', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const book = mockOrderBook(ctx, { mid: '3000', depth: 5, tickSize: '1' })
    expect(book.pair).toBe(DEFAULT_PAIR)
    expect(book.bids).toHaveLength(5)
    expect(book.asks).toHaveLength(5)

    for (let i = 1; i < book.bids.length; i++) {
      expect(new Decimal(book.bids[i].price).lt(book.bids[i - 1].price)).toBe(true)
    }
    for (let i = 1; i < book.asks.length; i++) {
      expect(new Decimal(book.asks[i].price).gt(book.asks[i - 1].price)).toBe(true)
    }

    const bestBid = new Decimal(book.bids[0].price)
    const bestAsk = new Decimal(book.asks[0].price)
    expect(bestBid.lt(bestAsk)).toBe(true)
    expect(midFromBook(book).equals(new Decimal('3000'))).toBe(true)
  })

  it('drops bids that would go below zero (small mid, deep depth)', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const book = mockOrderBook(ctx, { mid: '3', depth: 10, tickSize: '1' })
    for (const bid of book.bids) {
      expect(new Decimal(bid.price).gt(0)).toBe(true)
    }
  })
})

describe('mockOrderInfo', () => {
  it('uses ctx defaults when nothing is overridden', () => {
    const ctx = createFactoryContext({ seed: SEED, now: () => 1700000000 })
    const order = mockOrderInfo(ctx)
    expect(order.pair).toBe(DEFAULT_PAIR)
    expect(order.id.startsWith('mock-')).toBe(true)
    expect([Side.BUY, Side.SELL]).toContain(order.side)
    expect(order.submittedAtUnix).toBe(1700000000n)
    expect(order.remainingSize).toBe(order.size)
  })

  it('honours every override field', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const order = mockOrderInfo(ctx, {
      id: 'fixed-id',
      side: Side.SELL,
      price: '3000.50',
      size: '2.5',
      remainingSize: '1.25',
      pair: 'ETH/USDC',
      submittedAtUnix: 17n,
      expiresAtUnix: 42n,
      commitmentKey: 'forced',
    })
    expect(order.id).toBe('fixed-id')
    expect(order.side).toBe(Side.SELL)
    expect(order.price).toBe('3000.5')
    expect(order.size).toBe('2.5')
    expect(order.remainingSize).toBe('1.25')
    expect(order.submittedAtUnix).toBe(17n)
    expect(order.expiresAtUnix).toBe(42n)
    expect(order.commitmentKey).toBe('forced')
  })
})

describe('mockAuctionSummary', () => {
  it('draws clearing price within ±noise of mid', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const auction = mockAuctionSummary(ctx, { mid: '3000', noise: '1.5' })
    const cp = new Decimal(auction.clearingPrice)
    expect(cp.gte('2998.5')).toBe(true)
    expect(cp.lte('3001.5')).toBe(true)
  })

  it('respects an explicit clearingPrice', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const auction = mockAuctionSummary(ctx, { clearingPrice: '3142.00' })
    expect(auction.clearingPrice).toBe('3142')
  })

  it('emits int64 timestamp as bigint per proto3 JSON mapping', () => {
    const ctx = createFactoryContext({ seed: SEED, now: () => 1700000099 })
    expect(mockAuctionSummary(ctx).timestampUnix).toBe(1700000099n)
  })
})

describe('mockFill', () => {
  it('canonicalizes price and size into wire strings', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const fill = mockFill(ctx, {
      orderId: 'ord-1',
      side: Side.BUY,
      price: '3000.50',
      size: '0.500000',
    })
    expect(fill.orderId).toBe('ord-1')
    expect(fill.side).toBe(Side.BUY)
    expect(fill.price).toBe('3000.5')
    expect(fill.size).toBe('0.5')
  })
})

describe('mockBalances', () => {
  it('falls back to sane defaults and canonicalizes overrides', () => {
    const ctx = createFactoryContext({ seed: SEED })
    expect(mockBalances(ctx)).toEqual({ weth: '10', usdc: '25000' })
    expect(mockBalances(ctx, { weth: '0.5000', usdc: '1500.000' })).toEqual({
      weth: '0.5',
      usdc: '1500',
    })
  })
})

describe('scaleWireSize', () => {
  it('scales without leaking JS floats', () => {
    // 0.3 * 0.1 in binary floats drifts to 0.029999999...; the helper
    // routes through Decimal, so the wire string is exact.
    expect(scaleWireSize('0.3', '0.1', '0')).toBe('0.03')
  })

  it('floors at the provided minimum', () => {
    expect(scaleWireSize('0.0002', '0.1', '0.0001')).toBe('0.0001')
  })
})

describe('midFromBook', () => {
  it('returns the midpoint of best bid and best ask', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const book = mockOrderBook(ctx, { mid: '3000', depth: 3, tickSize: '2' })
    // best bid = 2998, best ask = 3002 → mid = 3000
    expect(midFromBook(book).toString()).toBe('3000')
  })

  it('throws on an empty side', () => {
    const ctx = createFactoryContext({ seed: SEED })
    const book = mockOrderBook(ctx, { mid: DEFAULT_MID, depth: 1 })
    book.bids = []
    expect(() => midFromBook(book)).toThrow(/empty/)
  })
})
