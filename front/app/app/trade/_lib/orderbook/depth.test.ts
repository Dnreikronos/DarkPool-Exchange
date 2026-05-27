import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import { PriceLevelSchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { computeDepthRows, formatDelta } from './depth'

function level(price: string, totalSize: string, orderCount = 1) {
  return create(PriceLevelSchema, { price, totalSize, orderCount })
}

describe('computeDepthRows', () => {
  it('returns empty rows and zero max when both sides are empty', () => {
    const out = computeDepthRows([], [])
    expect(out.bids).toEqual([])
    expect(out.asks).toEqual([])
    expect(out.maxCumulative).toBe('0')
  })

  it('accumulates bids from the top of book downward', () => {
    const bids = [level('100', '2'), level('99', '3'), level('98', '5')]
    const out = computeDepthRows(bids, [])
    expect(out.bids.map((r) => r.cumulative)).toEqual(['2', '5', '10'])
  })

  it('accumulates asks from the top of book upward', () => {
    const asks = [level('101', '1'), level('102', '4'), level('103', '5')]
    const out = computeDepthRows([], asks)
    expect(out.asks.map((r) => r.cumulative)).toEqual(['1', '5', '10'])
  })

  it('normalizes barFraction against the larger side', () => {
    const bids = [level('100', '5'), level('99', '5')] // cumulative max 10
    const asks = [level('101', '1'), level('102', '1')] // cumulative max 2
    const out = computeDepthRows(bids, asks)
    // bids max is 10 → that's the global max, so bids end at 1.0
    expect(out.bids.at(-1)?.barFraction).toBeCloseTo(1.0, 5)
    // asks end at 2 / 10 = 0.2
    expect(out.asks.at(-1)?.barFraction).toBeCloseTo(0.2, 5)
    expect(out.maxCumulative).toBe('10')
  })

  it('handles decimal sizes precisely (no float drift)', () => {
    const bids = [level('100', '0.1'), level('99', '0.2')]
    const out = computeDepthRows(bids, [])
    // 0.1 + 0.2 must equal 0.3 exactly through Decimal arithmetic
    expect(out.bids.at(-1)?.cumulative).toBe('0.3')
  })

  it('preserves level identity for click-through', () => {
    const a = level('100', '1')
    const b = level('99', '1')
    const out = computeDepthRows([a, b], [])
    expect(out.bids[0].level).toBe(a)
    expect(out.bids[1].level).toBe(b)
  })
})

describe('formatDelta', () => {
  it('returns na when previous is null', () => {
    expect(formatDelta('100.00', null)).toEqual({ text: '—', sign: 'na' })
  })

  it('returns na when previous is undefined', () => {
    expect(formatDelta('100.00', undefined)).toEqual({ text: '—', sign: 'na' })
  })

  it('flags a positive delta with a + prefix', () => {
    expect(formatDelta('100.00', '95.50')).toEqual({ text: '+4.50', sign: 'pos' })
  })

  it('flags a negative delta with a - prefix', () => {
    expect(formatDelta('95.50', '100.00')).toEqual({ text: '-4.50', sign: 'neg' })
  })

  it('flags zero delta', () => {
    expect(formatDelta('100.00', '100.00')).toEqual({ text: '0.00', sign: 'zero' })
  })

  it('respects the displayDp argument', () => {
    expect(formatDelta('100.123', '100', 3)).toEqual({ text: '+0.123', sign: 'pos' })
  })

  it('rounds half-up to displayDp', () => {
    expect(formatDelta('100.005', '100', 2)).toEqual({ text: '+0.01', sign: 'pos' })
  })
})
