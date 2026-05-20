import { describe, expect, it } from 'vitest'

import { createMockStore } from '../../../lib/mock-store'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  selectLatestClearingPrice,
  selectPortfolioFills,
  selectPortfolioSummary,
} from './usePortfolio'

const FROZEN_NOW = 1_700_000_000

function freshState(seed = 7) {
  return createMockStore({
    seed,
    now: () => FROZEN_NOW,
    mid: '3000',
    depth: 4,
    auctionHistory: 4,
  }).getState()
}

describe('selectPortfolioFills', () => {
  it('returns the store fillHistory newest-first', () => {
    const store = createMockStore({ seed: 1, now: () => FROZEN_NOW })
    store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '0.5' })
    store.getState().runAuction()
    const fills = selectPortfolioFills(store.getState())
    expect(fills).toBe(store.getState().fillHistory)
  })

  it('returns an empty array when no fills exist yet', () => {
    expect(selectPortfolioFills(freshState())).toHaveLength(0)
  })
})

describe('selectLatestClearingPrice', () => {
  it('returns the latest auction clearing price as a wire string', () => {
    const state = freshState()
    expect(state.recentAuctions.length).toBeGreaterThan(0)
    const latest = selectLatestClearingPrice(state)
    expect(latest).toBe(state.recentAuctions[0].clearingPrice)
  })

  it('returns null when no auctions have cleared', () => {
    const store = createMockStore({ seed: 1, now: () => FROZEN_NOW, auctionHistory: 0 })
    expect(selectLatestClearingPrice(store.getState())).toBeNull()
  })
})

describe('selectPortfolioSummary', () => {
  it('combines fills and latest clearing price into a summary', () => {
    const store = createMockStore({
      seed: 99,
      now: () => FROZEN_NOW,
      depth: 4,
      auctionHistory: 2,
    })
    store.getState().placeOrder({ side: Side.BUY, price: '5000', size: '1' })
    store.getState().runAuction()
    const summary = selectPortfolioSummary(store.getState())
    expect(summary.position.weth).toBe('1')
    expect(summary.mark).toBe(store.getState().recentAuctions[0].clearingPrice)
    // P&L = (mark - avgEntry) * 1 — depends on auction sampling but should be a finite string.
    expect(summary.unrealizedPnl).toMatch(/^-?\d+(\.\d+)?$/)
  })
})
