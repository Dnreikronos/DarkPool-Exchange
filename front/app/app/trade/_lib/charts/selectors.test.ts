import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import {
  GetOrderBookResponseSchema,
  PriceLevelSchema,
  AuctionSummarySchema,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'
import type {
  AuctionSummary,
  GetOrderBookResponse,
  PriceLevel,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'

import { TIMEFRAME_MS, buildDepthSeries, selectAuctionsInWindow } from './selectors'

function level(price: string, totalSize: string, orderCount = 1): PriceLevel {
  return create(PriceLevelSchema, { price, totalSize, orderCount })
}

function book(bids: PriceLevel[], asks: PriceLevel[]): GetOrderBookResponse {
  return create(GetOrderBookResponseSchema, { pair: 'ETH/USDC', bids, asks })
}

function auction(timestampUnix: number, clearingPrice: string): AuctionSummary {
  return create(AuctionSummarySchema, {
    auctionId: `a-${timestampUnix}`,
    pair: 'ETH/USDC',
    clearingPrice,
    matchedVolume: '1',
    matchCount: 1,
    timestampUnix: BigInt(timestampUnix),
  })
}

describe('buildDepthSeries', () => {
  it('cumulates bid sizes outward from best bid (highest price first)', () => {
    const series = buildDepthSeries(
      book([level('3000', '1'), level('2999', '2'), level('2998', '0.5')], [level('3001', '1')])
    )
    expect(series.bids.map((p) => [p.price, p.cumulative])).toEqual([
      [3000, 1],
      [2999, 3],
      [2998, 3.5],
    ])
  })

  it('cumulates ask sizes outward from best ask (lowest price first)', () => {
    const series = buildDepthSeries(
      book([level('2999', '1')], [level('3001', '1.5'), level('3002', '0.5'), level('3003', '3')])
    )
    expect(series.asks.map((p) => [p.price, p.cumulative])).toEqual([
      [3001, 1.5],
      [3002, 2],
      [3003, 5],
    ])
  })

  it('computes mid price as midpoint of best bid and best ask', () => {
    const series = buildDepthSeries(book([level('2999', '1')], [level('3001', '1')]))
    expect(series.midPrice).toBe(3000)
    expect(series.midPriceStr).toBe('3000')
  })

  it('tracks max cumulative across both sides for shared y-domain', () => {
    const series = buildDepthSeries(
      book([level('2999', '1'), level('2998', '0.5')], [level('3001', '4')])
    )
    expect(series.maxCumulative).toBe(4)
  })

  it('returns the spread between best ask and best bid', () => {
    const series = buildDepthSeries(book([level('2999.5', '1')], [level('3000.25', '1')]))
    expect(series.spread).toBe(0.75)
  })

  it('returns null mid and spread when either side is empty', () => {
    const onlyBids = buildDepthSeries(book([level('2999', '1')], []))
    expect(onlyBids.midPrice).toBeNull()
    expect(onlyBids.midPriceStr).toBeNull()
    expect(onlyBids.spread).toBeNull()
    expect(onlyBids.asks).toEqual([])

    const onlyAsks = buildDepthSeries(book([], [level('3001', '1')]))
    expect(onlyAsks.midPrice).toBeNull()
    expect(onlyAsks.bids).toEqual([])
  })

  it('returns empty arrays and null mid for a fully empty book', () => {
    const series = buildDepthSeries(book([], []))
    expect(series.bids).toEqual([])
    expect(series.asks).toEqual([])
    expect(series.midPrice).toBeNull()
    expect(series.maxCumulative).toBe(0)
  })

  it('keeps the canonical wire string alongside the plotting number', () => {
    const series = buildDepthSeries(book([level('2999.5', '1.5')], [level('3001.25', '0.5')]))
    expect(series.bids[0]).toMatchObject({
      price: 2999.5,
      priceStr: '2999.5',
      cumulative: 1.5,
      cumulativeStr: '1.5',
    })
    expect(series.asks[0]).toMatchObject({
      price: 3001.25,
      priceStr: '3001.25',
      cumulative: 0.5,
      cumulativeStr: '0.5',
    })
  })

  it('cumulates with decimal precision (no float drift)', () => {
    const series = buildDepthSeries(
      book([level('3000', '0.1'), level('2999', '0.2')], [level('3001', '0.1')])
    )
    // 0.1 + 0.2 = 0.3 in Decimal; without Decimal we'd see 0.30000000000000004
    expect(series.bids[1].cumulativeStr).toBe('0.3')
  })
})

describe('TIMEFRAME_MS', () => {
  it('exposes the three preset window sizes', () => {
    expect(TIMEFRAME_MS['1m']).toBe(60_000)
    expect(TIMEFRAME_MS['5m']).toBe(5 * 60_000)
    expect(TIMEFRAME_MS['1h']).toBe(60 * 60_000)
  })
})

describe('selectAuctionsInWindow', () => {
  it('keeps auctions whose timestamp falls inside the window', () => {
    const now = 1_700_000_000
    const points = selectAuctionsInWindow(
      [auction(now - 10, '3000'), auction(now - 70, '2999'), auction(now - 30, '3001')],
      TIMEFRAME_MS['1m'],
      now
    )
    expect(points.map((p) => p.value)).toEqual([3001, 3000])
  })

  it('returns chronological order (oldest first) for chart ingestion', () => {
    const now = 1_700_000_000
    const points = selectAuctionsInWindow(
      [auction(now - 5, '3010'), auction(now - 30, '3005'), auction(now - 55, '3000')],
      TIMEFRAME_MS['1m'],
      now
    )
    expect(points.map((p) => p.time)).toEqual([now - 55, now - 30, now - 5])
  })

  it('drops auctions older than the window', () => {
    const now = 1_700_000_000
    const points = selectAuctionsInWindow(
      [auction(now - 90, '2999'), auction(now - 30, '3001')],
      TIMEFRAME_MS['1m'],
      now
    )
    expect(points).toHaveLength(1)
    expect(points[0].value).toBe(3001)
  })

  it('collapses duplicate timestamps so lightweight-charts does not throw', () => {
    const now = 1_700_000_000
    const points = selectAuctionsInWindow(
      [
        auction(now - 10, '3002'), // newest at t-10
        auction(now - 10, '3000'), // older entry at the same second
        auction(now - 30, '2998'),
      ],
      TIMEFRAME_MS['1m'],
      now
    )
    expect(points.map((p) => [p.time, p.value])).toEqual([
      [now - 30, 2998],
      [now - 10, 3002], // newest wins
    ])
  })

  it('returns [] when nothing falls inside the window', () => {
    const now = 1_700_000_000
    expect(selectAuctionsInWindow([auction(now - 1_000, '3000')], TIMEFRAME_MS['1m'], now)).toEqual(
      []
    )
  })

  it('returns [] when given no auctions', () => {
    expect(selectAuctionsInWindow([], TIMEFRAME_MS['1m'], 1_700_000_000)).toEqual([])
  })
})
