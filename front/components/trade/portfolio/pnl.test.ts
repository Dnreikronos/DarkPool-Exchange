import { describe, expect, it } from 'vitest'

import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '../../../lib/mock-store'

import {
  EPSILON_WETH,
  EPSILON_USDC,
  computeDivergence,
  computePosition,
  computeRealizedPnl,
  computeSummary,
  computeUnrealizedPnl,
  weightedAvgEntry,
} from './pnl'

function fill(side: Side, price: string, size: string, ts = 1n): Fill {
  return {
    fillId: `f-${ts}-${side}-${price}-${size}`,
    orderId: `o-${ts}`,
    auctionId: `a-${ts}`,
    side,
    price,
    size,
    timestampUnix: ts,
  }
}

describe('computePosition', () => {
  it('returns zero deltas for an empty fill set', () => {
    const p = computePosition([])
    expect(p.weth).toBe('0')
    expect(p.usdc).toBe('0')
  })

  it('treats a BUY as +size WETH, -size*price USDC', () => {
    const p = computePosition([fill(Side.BUY, '3000', '1')])
    expect(p.weth).toBe('1')
    expect(p.usdc).toBe('-3000')
  })

  it('treats a SELL as -size WETH, +size*price USDC', () => {
    const p = computePosition([fill(Side.SELL, '3050', '2')])
    expect(p.weth).toBe('-2')
    expect(p.usdc).toBe('6100')
  })

  it('accumulates mixed fills', () => {
    const p = computePosition([
      fill(Side.BUY, '3000', '1'),
      fill(Side.BUY, '3010', '0.5'),
      fill(Side.SELL, '3050', '0.5'),
    ])
    // weth: 1 + 0.5 - 0.5 = 1
    // usdc: -3000 - 1505 + 1525 = -2980
    expect(p.weth).toBe('1')
    expect(p.usdc).toBe('-2980')
  })
})

describe('weightedAvgEntry', () => {
  it('returns null for empty fills', () => {
    expect(weightedAvgEntry([], Side.BUY)).toBeNull()
  })

  it('returns the single-fill price when only one entry exists', () => {
    expect(weightedAvgEntry([fill(Side.BUY, '3000', '1')], Side.BUY)).toBe('3000')
  })

  it('returns the size-weighted average price across multiple entries', () => {
    // 1 ETH @ 3000 + 3 ETH @ 3200 = 12600 / 4 = 3150
    const fills = [fill(Side.BUY, '3000', '1'), fill(Side.BUY, '3200', '3')]
    expect(weightedAvgEntry(fills, Side.BUY)).toBe('3150')
  })

  it('only considers fills on the requested side', () => {
    const fills = [
      fill(Side.BUY, '3000', '1'),
      fill(Side.SELL, '9999', '1'),
      fill(Side.BUY, '3200', '3'),
    ]
    expect(weightedAvgEntry(fills, Side.BUY)).toBe('3150')
  })

  it('returns null when no fills on the requested side', () => {
    expect(weightedAvgEntry([fill(Side.BUY, '3000', '1')], Side.SELL)).toBeNull()
  })
})

describe('computeUnrealizedPnl', () => {
  it('returns null when net position is zero', () => {
    const fills = [fill(Side.BUY, '3000', '1'), fill(Side.SELL, '3050', '1')]
    expect(computeUnrealizedPnl(fills, '3100')).toBeNull()
  })

  it('returns null with no fills', () => {
    expect(computeUnrealizedPnl([], '3100')).toBeNull()
  })

  it('returns null when mark price is missing', () => {
    expect(computeUnrealizedPnl([fill(Side.BUY, '3000', '1')], null)).toBeNull()
  })

  it('computes positive P&L for a long when mark > avg entry', () => {
    // 1 ETH @ 3000, mark 3100 → +100 USDC
    expect(computeUnrealizedPnl([fill(Side.BUY, '3000', '1')], '3100')).toBe('100')
  })

  it('computes negative P&L for a long when mark < avg entry', () => {
    expect(computeUnrealizedPnl([fill(Side.BUY, '3000', '1')], '2950')).toBe('-50')
  })

  it('computes positive P&L for a short when mark < avg entry', () => {
    // sold 1 ETH @ 3000, mark 2950 → +50 USDC
    expect(computeUnrealizedPnl([fill(Side.SELL, '3000', '1')], '2950')).toBe('50')
  })

  it('uses only the relevant entry side to derive avg entry on a net-long book', () => {
    // 2 BUY @ 3000, 1 SELL @ 9999 → net +1 WETH; avg entry from buys = 3000; mark 3100
    // pnl = (3100 - 3000) * 1 = 100
    const fills = [fill(Side.BUY, '3000', '2'), fill(Side.SELL, '9999', '1')]
    expect(computeUnrealizedPnl(fills, '3100')).toBe('100')
  })
})

describe('computeRealizedPnl', () => {
  it('returns 0 when no round trips exist', () => {
    expect(computeRealizedPnl([fill(Side.BUY, '3000', '1')])).toBe('0')
  })

  it('captures a simple round trip', () => {
    // buy 1 @ 3000, sell 1 @ 3050 → cash delta of +50 USDC
    const fills = [fill(Side.BUY, '3000', '1'), fill(Side.SELL, '3050', '1')]
    expect(computeRealizedPnl(fills)).toBe('50')
  })

  it('matches USDC delta when net WETH returns to zero', () => {
    const fills = [
      fill(Side.BUY, '3000', '0.5'),
      fill(Side.BUY, '3010', '0.5'),
      fill(Side.SELL, '3050', '1'),
    ]
    // usdc: -1500 - 1505 + 3050 = 45
    expect(computeRealizedPnl(fills)).toBe('45')
  })

  it('returns 0 while a position is still open', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    expect(computeRealizedPnl(fills)).toBe('0')
  })
})

describe('computeDivergence', () => {
  it('reports no divergence when expected delta matches internal balance', () => {
    // No fills, no balance → no divergence.
    const result = computeDivergence([], { weth: '0', usdc: '0' })
    expect(result.diverged).toBe(false)
    expect(result.expected.weth).toBe('0')
    expect(result.expected.usdc).toBe('0')
  })

  it('flags divergence when fills exist but internal balance is zero', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    const result = computeDivergence(fills, { weth: '0', usdc: '0' })
    expect(result.diverged).toBe(true)
    expect(result.expected.weth).toBe('1')
    expect(result.expected.usdc).toBe('-3000')
    expect(result.actual.weth).toBe('0')
    expect(result.actual.usdc).toBe('0')
  })

  it('does not flag divergence within epsilon', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    // Internal balance reflects the fill within epsilon
    const result = computeDivergence(fills, { weth: '1', usdc: '-3000' })
    expect(result.diverged).toBe(false)
  })

  it('flags divergence when difference exceeds epsilon', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    // 0.001 WETH off — exceeds EPSILON_WETH
    const result = computeDivergence(fills, { weth: '1.001', usdc: '-3000' })
    expect(result.diverged).toBe(true)
  })

  it('handles only-WETH divergence', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    const result = computeDivergence(fills, { weth: '0.5', usdc: '-3000' })
    expect(result.diverged).toBe(true)
  })

  it('exposes the epsilon constants as part of the public contract', () => {
    expect(EPSILON_WETH).toBeDefined()
    expect(EPSILON_USDC).toBeDefined()
  })
})

describe('computeSummary', () => {
  it('returns a fully populated summary on a round trip with mark', () => {
    const fills = [fill(Side.BUY, '3000', '1'), fill(Side.SELL, '3050', '0.5')]
    const summary = computeSummary(fills, '3100')
    expect(summary.position.weth).toBe('0.5')
    expect(summary.position.usdc).toBe('-1475')
    expect(summary.avgEntry).toBe('3000')
    expect(summary.unrealizedPnl).toBe('50') // (3100-3000)*0.5
    expect(summary.realizedPnl).toBe('0') // still open
    expect(summary.mark).toBe('3100')
  })

  it('returns null mark + null unrealized when no auctions yet', () => {
    const fills = [fill(Side.BUY, '3000', '1')]
    const summary = computeSummary(fills, null)
    expect(summary.mark).toBeNull()
    expect(summary.unrealizedPnl).toBeNull()
    expect(summary.avgEntry).toBe('3000')
  })
})
