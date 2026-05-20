'use client'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { DepthRow } from './DepthRow'

import type { DepthRow as DepthRowData } from '../../_lib/orderbook/depth'

export type DepthTableSide = Side.BUY | Side.SELL

export interface DepthTableProps {
  rows: DepthRowData[]
  side: DepthTableSide
  /**
   * Reverses the render order. Asks display from worst→best top-to-bottom
   * (so best ask sits adjacent to the spread row beneath them); bids run
   * best→worst.
   */
  reverse?: boolean
  onSelect?: (price: string, side: DepthTableSide) => void
}

/**
 * One side of the book. The first row (index 0 after optional reverse) is
 * the side closest to the spread — gets one typography step up so the
 * top of book reads with hierarchy.
 */
export function DepthTable({ rows, side, reverse = false, onSelect }: DepthTableProps) {
  const ordered = reverse ? [...rows].reverse() : rows
  const isBid = side === Side.BUY
  const bestIndex = reverse ? ordered.length - 1 : 0

  return (
    <div role="group" aria-label={isBid ? 'Bids' : 'Asks'} className="flex flex-col">
      <div className="flex items-center gap-2 px-4 py-1">
        <span className="font-mono text-label-md uppercase text-brand-muted">
          [ {isBid ? 'BIDS' : 'ASKS'} ]
        </span>
      </div>
      <ul className="flex flex-col">
        {ordered.map((row, i) => (
          <li key={row.level.price}>
            <DepthRow row={row} side={side} emphasized={i === bestIndex} onSelect={onSelect} />
          </li>
        ))}
      </ul>
    </div>
  )
}
