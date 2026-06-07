import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, describe, expect, it } from 'vitest'

import { createMockStore, mockStore } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'

import { selectLatestClearingPrice, usePortfolio } from './usePortfolio'

const FROZEN_NOW = 1_700_000_000

const FLAT = { weth: '0', usdc: '0' }

function freshState(seed = 7) {
  return createMockStore({
    seed,
    now: () => FROZEN_NOW,
    mid: '3000',
    depth: 4,
    auctionHistory: 4,
  }).getState()
}

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

function Probe(): JSX.Element {
  const { fills, summary } = usePortfolio(FLAT)
  return (
    <span>
      fills:{fills.length};weth:{summary.position.weth}
    </span>
  )
}

describe('usePortfolio (persistent-history contract)', () => {
  afterEach(() => {
    walletStore.disconnect()
    mockStore.getState().reset()
  })

  it('sources fills from the persistent history, not the mock store singleton', () => {
    // Force a fill into the in-memory mock store. The portfolio must NOT
    // see it directly — fills only count once they land in IndexedDB
    // (via the HistoryBoot mirror), which the SSR commit cannot read.
    walletStore.connect()
    mockStore.getState().placeOrder({ side: Side.BUY, price: '1000000', size: '1' })
    mockStore.getState().runAuction()
    expect(mockStore.getState().fillHistory.length).toBeGreaterThan(0)

    expect(renderToStaticMarkup(<Probe />)).toContain('fills:0')
  })

  it('derives a flat summary from an empty history', () => {
    expect(renderToStaticMarkup(<Probe />)).toContain('weth:0')
  })
})
