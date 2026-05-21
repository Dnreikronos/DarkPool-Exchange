'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'

import { formatBatch, formatFillTimestamp } from '../_lib/format'

export interface FillHistoryRowProps {
  fill: Fill
}

function FillHistoryRowImpl({ fill }: FillHistoryRowProps): JSX.Element {
  const sideLabel = fill.side === Side.BUY ? '[ BUY ]' : '[ SELL ]'
  return (
    <li className="border-b border-brand-border last:border-b-0">
      <div
        className="grid grid-cols-[10rem_4rem_minmax(0,1fr)_minmax(0,1fr)_minmax(0,9rem)] items-center gap-3 px-4 py-3 font-mono text-body-sm tabular-nums text-brand-fg"
        aria-label={`Fill ${fill.fillId}, ${fill.side === Side.BUY ? 'buy' : 'sell'} ${fill.size} at ${fill.price}`}
      >
        <span className="text-brand-muted">{formatFillTimestamp(fill.timestampUnix)}</span>
        <span className="text-brand-fg">{sideLabel}</span>
        <NumericText value={fill.price} kind="price" align="right" className="text-brand-fg" />
        <NumericText value={fill.size} kind="size" align="right" className="text-brand-fg" />
        <span className="text-right text-brand-muted">[ {formatBatch(fill.auctionId)} ]</span>
      </div>
    </li>
  )
}

export const FillHistoryRow = React.memo(FillHistoryRowImpl)
FillHistoryRow.displayName = 'FillHistoryRow'
