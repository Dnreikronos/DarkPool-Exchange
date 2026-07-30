'use client'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { NumericText } from '@/components/NumericText'

import type { DepthRow as DepthRowData } from '../../_lib/orderbook/depth'

export interface DepthRowProps {
  row: DepthRowData
  side: Side.BUY | Side.SELL
  /** Best-of-book row gets one typography step up. */
  emphasized?: boolean
  /**
   * Marks the row as belonging to the connected trader — adds a 2px white
   * left edge so the user can spot their resting orders inside the book.
   * Driven by F1.10 (#77) via the `userPrices` prop chain on OrderBook.
   */
  mine?: boolean
  onSelect?: (price: string, side: Side.BUY | Side.SELL) => void
}

/**
 * Single book row. The depth bar is the row's background — a horizontal
 * fill scaled to `barFraction`. Bids use `primary` at 10% opacity; asks
 * use `secondary` at 20% (per DESIGN-INSPIRATIONS bid/ask tension table).
 * No semantic color anywhere.
 */
export function DepthRow({ row, side, emphasized = false, mine = false, onSelect }: DepthRowProps) {
  const isBid = side === Side.BUY
  const bar = isBid ? 'bg-brand-fg/[0.08]' : 'bg-brand-muted/20'
  const sizeClass = emphasized ? 'text-body-md' : 'text-body-sm'
  const handleClick = onSelect ? () => onSelect(row.level.price, side) : undefined
  const clickable = handleClick !== undefined

  const Tag = clickable ? 'button' : 'div'
  return (
    <Tag
      type={clickable ? 'button' : undefined}
      onClick={handleClick}
      className={`relative grid w-full grid-cols-3 items-center gap-2 px-4 py-1 text-left font-mono transition-colors duration-100 hover:bg-brand-surface focus-visible:bg-brand-surface focus-visible:outline focus-visible:outline-1 focus-visible:outline-brand-accent focus-visible:outline-offset-[-1px] ${sizeClass}`}
      // An aria-label replaces the button's content as its accessible
      // name, so the size and total columns below are unreachable unless
      // they are named here too (#205).
      aria-label={
        clickable
          ? `${isBid ? 'Bid' : 'Ask'} ${row.level.price}, size ${row.level.totalSize}, total ${row.cumulative}`
          : undefined
      }
      data-mine={mine || undefined}
    >
      <span
        aria-hidden="true"
        className={`pointer-events-none absolute inset-y-0 right-0 ${bar}`}
        style={{ width: `${Math.min(100, Math.max(0, row.barFraction * 100))}%` }}
      />
      {mine && (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-y-0 left-0 w-0.5 bg-brand-fg"
        />
      )}
      <NumericText
        value={row.level.price}
        kind="price"
        align="left"
        className="relative z-10 text-brand-fg"
      />
      <NumericText
        value={row.level.totalSize}
        kind="size"
        align="right"
        className="relative z-10 text-brand-muted"
      />
      <NumericText
        value={row.cumulative}
        kind="size"
        align="right"
        className="relative z-10 text-brand-fg"
      />
    </Tag>
  )
}
