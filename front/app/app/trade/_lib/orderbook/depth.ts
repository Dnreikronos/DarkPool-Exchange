import { Decimal } from '@/lib/units'

import type { PriceLevel } from '@/lib/sdk/orderbook'

export interface DepthRow {
  level: PriceLevel
  /** Cumulative wire-string size from the top of book down to (and including) this level. */
  cumulative: string
  /** [0,1] — share of `maxCumulative`, drives the depth-bar width. */
  barFraction: number
}

export interface DepthRows {
  bids: DepthRow[]
  asks: DepthRow[]
  /** Larger of the two sides' deepest cumulative size. Use it to keep bid + ask bars on the same scale. */
  maxCumulative: string
}

/**
 * Accumulates `totalSize` along each side. Bids and asks arrive pre-sorted
 * (bids high→low, asks low→high — best of book at index 0), so cumulative
 * sums grow away from the spread.
 *
 * Both sides share a single normalization denominator so the deeper side
 * doesn't visually dwarf the thinner one — a 10-ETH ask shouldn't render
 * the same width as a 1-ETH bid just because their sides happen to clear
 * differently.
 */
export function computeDepthRows(bids: PriceLevel[], asks: PriceLevel[]): DepthRows {
  const bidsAcc = accumulate(bids)
  const asksAcc = accumulate(asks)
  const bidsMax = bidsAcc.at(-1)?.runningTotal ?? new Decimal(0)
  const asksMax = asksAcc.at(-1)?.runningTotal ?? new Decimal(0)
  const max = bidsMax.gte(asksMax) ? bidsMax : asksMax
  const maxStr = max.toFixed()
  const bidRows = bidsAcc.map((entry) => toRow(entry, max))
  const askRows = asksAcc.map((entry) => toRow(entry, max))
  return { bids: bidRows, asks: askRows, maxCumulative: maxStr }
}

interface AccEntry {
  level: PriceLevel
  runningTotal: Decimal
}

function accumulate(levels: PriceLevel[]): AccEntry[] {
  const out: AccEntry[] = []
  let running = new Decimal(0)
  for (const level of levels) {
    running = running.plus(level.totalSize)
    out.push({ level, runningTotal: running })
  }
  return out
}

function toRow(entry: AccEntry, max: Decimal): DepthRow {
  const fraction = max.gt(0) ? entry.runningTotal.div(max).toNumber() : 0
  return {
    level: entry.level,
    cumulative: entry.runningTotal.toFixed(),
    barFraction: fraction,
  }
}

export type DeltaSign = 'pos' | 'neg' | 'zero' | 'na'

export interface FormattedDelta {
  text: string
  sign: DeltaSign
}

/**
 * Signed-prefix delta between two wire-string prices. Returns `na` when
 * there's no prior reference (first ever auction). The brutalist palette
 * has no green / red — consumers map `sign` to typography weight or
 * opacity, never to hue.
 */
export function formatDelta(
  current: string,
  previous: string | null | undefined,
  displayDp = 2
): FormattedDelta {
  if (previous == null) return { text: '—', sign: 'na' }
  const diff = new Decimal(current).minus(previous)
  const rounded = diff.toDecimalPlaces(displayDp, Decimal.ROUND_HALF_UP)
  if (rounded.isZero()) {
    return { text: rounded.toFixed(displayDp), sign: 'zero' }
  }
  const fixed = rounded.toFixed(displayDp)
  if (rounded.isPositive()) {
    return { text: `+${fixed}`, sign: 'pos' }
  }
  // toFixed() already includes the leading minus sign for negatives.
  return { text: fixed, sign: 'neg' }
}
