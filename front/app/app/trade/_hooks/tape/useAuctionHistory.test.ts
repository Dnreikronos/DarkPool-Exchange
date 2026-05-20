import { describe, expect, it } from 'vitest'

import { createMockStore } from '@/lib/mock-store'

import { DEFAULT_AUCTION_HISTORY_LIMIT, selectLatestAuctions } from './useAuctionHistory'

const FROZEN_NOW_SECONDS = 1_700_000_000
const SEED = 11

function freshState(seed = SEED) {
  return createMockStore({
    seed,
    now: () => FROZEN_NOW_SECONDS,
    mid: '3000',
    depth: 4,
    auctionHistory: 8,
  }).getState()
}

describe('selectLatestAuctions', () => {
  it('returns at most `limit` rows, newest first', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state, 5)
    expect(rows).toHaveLength(5)
    for (let i = 0; i < rows.length - 1; i++) {
      expect(rows[i].timestampUnix >= rows[i + 1].timestampUnix).toBe(true)
    }
  })

  it('returns the full history when limit exceeds available rows', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state, 999)
    expect(rows).toHaveLength(state.recentAuctions.length)
  })

  it('returns referentially stable elements across calls (memo seam)', () => {
    const state = freshState()
    const a = selectLatestAuctions(state, 3)
    const b = selectLatestAuctions(state, 3)
    expect(a[0]).toBe(b[0])
    expect(a[1]).toBe(b[1])
  })

  it('returns the same array reference when the limit covers all rows', () => {
    const state = freshState()
    const a = selectLatestAuctions(state, 999)
    const b = selectLatestAuctions(state, 999)
    expect(a).toBe(b)
  })

  it('clamps negative or fractional limits without throwing', () => {
    const state = freshState()
    expect(selectLatestAuctions(state, -5)).toHaveLength(0)
    expect(selectLatestAuctions(state, 2.7)).toHaveLength(2)
  })

  it('honors the default limit when called without an explicit value', () => {
    const state = freshState()
    const rows = selectLatestAuctions(state)
    expect(rows.length).toBeLessThanOrEqual(DEFAULT_AUCTION_HISTORY_LIMIT)
  })
})
