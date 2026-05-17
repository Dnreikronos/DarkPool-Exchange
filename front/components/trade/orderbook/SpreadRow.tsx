'use client'

import { Decimal } from '../../../lib/units'
import { NumericText } from '../../NumericText'

export interface SpreadRowProps {
  bestBid: string | null
  bestAsk: string | null
}

/**
 * Single-row band between asks and bids reporting the spread (absolute and
 * as a basis-point ratio of the mid). Renders an em-dash placeholder when
 * either side of the book is empty.
 */
export function SpreadRow({ bestBid, bestAsk }: SpreadRowProps) {
  const { absolute, bps } = computeSpread(bestBid, bestAsk)
  return (
    <div
      className="flex items-center justify-between border-y border-brand-border bg-brand-surface px-4 py-2"
      aria-label="Spread"
    >
      <span className="font-mono text-label-md uppercase text-brand-muted">[ SPREAD ]</span>
      <div className="flex items-center gap-3">
        {absolute !== null ? (
          <NumericText value={absolute} kind="price" align="right" className="text-brand-fg" />
        ) : (
          <span className="font-mono text-body-sm text-brand-muted">—</span>
        )}
        <span className="font-mono text-body-sm text-brand-muted">
          {bps !== null ? `${bps} bps` : '—'}
        </span>
      </div>
    </div>
  )
}

function computeSpread(
  bestBid: string | null,
  bestAsk: string | null
): { absolute: string | null; bps: string | null } {
  if (bestBid === null || bestAsk === null) return { absolute: null, bps: null }
  const bid = new Decimal(bestBid)
  const ask = new Decimal(bestAsk)
  const diff = ask.minus(bid)
  if (!diff.isFinite() || diff.isNegative()) return { absolute: null, bps: null }
  const mid = ask.plus(bid).div(2)
  const bps = mid.gt(0) ? diff.div(mid).times(10000).toDecimalPlaces(1).toFixed(1) : null
  return { absolute: diff.toFixed(2), bps }
}
