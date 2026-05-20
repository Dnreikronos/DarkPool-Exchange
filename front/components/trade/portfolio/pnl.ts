// Pure P&L math for the portfolio panel. No React, no DOM.
//
// All inputs come in as wire-string decimals (Fill.price, Fill.size,
// AuctionSummary.clearingPrice — see docs/adr/0002-decimals.md). All
// outputs are wire-string decimals too. Internal math goes through
// Decimal to avoid binary-float drift.

import { Decimal } from '../../../lib/units'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '../../../lib/mock-store'

export interface Position {
  /** Net WETH delta from fills (positive = long). */
  weth: string
  /** Net USDC delta from fills (negative when net long). */
  usdc: string
}

export interface DivergenceResult {
  /** Whether the fill-derived position diverges from the on-chain internal balance. */
  diverged: boolean
  /** Position derived from the local fill history. */
  expected: Position
  /** Internal balance reported by the wallet/pool. */
  actual: Position
}

export interface PortfolioSummary {
  /** Net position from accumulating fills. */
  position: Position
  /** Size-weighted avg entry price on the side that opened the position, or null when flat. */
  avgEntry: string | null
  /** Latest clearing price used as mark; null when no auction has cleared. */
  mark: string | null
  /** Unrealized P&L vs the mark; null when flat or no mark. */
  unrealizedPnl: string | null
  /** Realized P&L (cash delta whenever a round-trip closes). */
  realizedPnl: string
}

/**
 * Divergence epsilons. We tolerate one unit of display precision so a
 * 4dp rounding mismatch between the wallet store and the fill-derived
 * position doesn't trip the banner.
 */
export const EPSILON_WETH = new Decimal('0.0001')
export const EPSILON_USDC = new Decimal('0.01')

function toDec(v: string): Decimal {
  return new Decimal(v)
}

/**
 * Accumulate per-side deltas. A BUY consumes USDC and produces WETH; a
 * SELL is the reverse. The mock store never emits a fill outside this
 * binary, so we don't need to handle Side.UNSPECIFIED here.
 */
export function computePosition(fills: readonly Fill[]): Position {
  let weth = new Decimal(0)
  let usdc = new Decimal(0)
  for (const f of fills) {
    const size = toDec(f.size)
    const notional = size.times(f.price)
    if (f.side === Side.BUY) {
      weth = weth.plus(size)
      usdc = usdc.minus(notional)
    } else {
      weth = weth.minus(size)
      usdc = usdc.plus(notional)
    }
  }
  return { weth: weth.toFixed(), usdc: usdc.toFixed() }
}

/**
 * Size-weighted average price across fills on `side`. Returns null when
 * no fill on that side exists.
 */
export function weightedAvgEntry(fills: readonly Fill[], side: Side): string | null {
  let totalSize = new Decimal(0)
  let totalNotional = new Decimal(0)
  for (const f of fills) {
    if (f.side !== side) continue
    const size = toDec(f.size)
    totalSize = totalSize.plus(size)
    totalNotional = totalNotional.plus(size.times(f.price))
  }
  if (totalSize.isZero()) return null
  return totalNotional.div(totalSize).toFixed()
}

/**
 * Unrealized P&L for the open position vs the mark. Returns null when
 * the book is flat or no mark is available.
 *
 * The MVP uses naive weighted-avg cost basis on the entry side — proper
 * FIFO/LIFO accounting is out of scope for the mock and lands behind the
 * real Phase 2 surface.
 */
export function computeUnrealizedPnl(
  fills: readonly Fill[],
  markPrice: string | null
): string | null {
  if (markPrice === null) return null
  const position = toDec(computePosition(fills).weth)
  if (position.isZero()) return null
  const entrySide = position.isPositive() ? Side.BUY : Side.SELL
  const avgEntry = weightedAvgEntry(fills, entrySide)
  if (avgEntry === null) return null
  const mark = toDec(markPrice)
  const avg = toDec(avgEntry)
  // (mark - avg) * position; for shorts position is negative, so the sign flips automatically.
  return mark.minus(avg).times(position).toFixed()
}

/**
 * Realized P&L is the USDC delta locked in once the position returns to
 * (or crosses) flat. In the MVP we approximate it with the cash delta
 * net of the still-open notional, valued at the most recent entry price.
 *
 * Pragmatic shortcut: when the position is currently flat, the entire
 * USDC delta is realized; while a position is open, realized P&L is
 * zero — the open exposure hides the rest. This is the same shortcut
 * Hyperliquid takes in its positions panel and is consistent with the
 * avg-entry weighting above.
 */
export function computeRealizedPnl(fills: readonly Fill[]): string {
  const position = computePosition(fills)
  if (!toDec(position.weth).isZero()) return '0'
  return position.usdc
}

/**
 * Compare the fill-derived position against the pool's internal balance.
 *
 * Phase 1 limitation: `internal` is the wallet store's `internalBalances`
 * snapshot, which has no deposit/withdrawal history layered in (#72 ships
 * that). The strict-equality check below therefore fires whenever the
 * trader has either (a) any non-zero pool balance with no fills, or
 * (b) any fills with no matching pool balance. Both are *symptoms* of
 * the same gap — the local session can't reconstruct
 * `deposits + fills - withdrawals`. The banner copy in
 * `DivergenceBanner.tsx` is intentionally framed around the symptom so
 * we don't claim "lost history" when the real cause is "deposits not
 * yet tracked".
 *
 * Phase 2 path: feed `internal` through `deposits + fills - withdrawals`
 * before calling, and the same function narrows to its intended meaning
 * — lost fill history — without a signature change.
 */
export function computeDivergence(fills: readonly Fill[], internal: Position): DivergenceResult {
  const expected = computePosition(fills)
  const wethDiff = toDec(expected.weth).minus(internal.weth).abs()
  const usdcDiff = toDec(expected.usdc).minus(internal.usdc).abs()
  const diverged = wethDiff.gt(EPSILON_WETH) || usdcDiff.gt(EPSILON_USDC)
  return { diverged, expected, actual: { ...internal } }
}

export function computeSummary(fills: readonly Fill[], markPrice: string | null): PortfolioSummary {
  const position = computePosition(fills)
  const netWeth = toDec(position.weth)
  const entrySide = netWeth.isZero() ? null : netWeth.isPositive() ? Side.BUY : Side.SELL
  const avgEntry = entrySide === null ? null : weightedAvgEntry(fills, entrySide)
  const unrealizedPnl = computeUnrealizedPnl(fills, markPrice)
  const realizedPnl = computeRealizedPnl(fills)
  return {
    position,
    avgEntry,
    mark: markPrice,
    unrealizedPnl,
    realizedPnl,
  }
}
