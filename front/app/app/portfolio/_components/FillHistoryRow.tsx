'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'
import { shortTxHash, type SettlementLink } from '@/lib/settlement'

import { formatBatch, formatFillTimestamp } from '../_lib/format'

export interface FillHistoryRowProps {
  fill: Fill
  /**
   * On-chain settlement correlated to this fill's auction (#100).
   * Null/undefined until a BatchSettled event is linked.
   */
  link?: SettlementLink | null
}

function FillHistoryRowImpl({ fill, link = null }: FillHistoryRowProps): JSX.Element {
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
        <BatchCell auctionId={fill.auctionId} link={link} />
      </div>
    </li>
  )
}

/**
 * BATCH column: the settlement tx (linked to the block explorer when one
 * exists) once correlated, the auction-id placeholder until then.
 */
function BatchCell({
  auctionId,
  link,
}: {
  auctionId: string
  link: SettlementLink | null
}): JSX.Element {
  if (!link) {
    return <span className="text-right text-brand-muted">[ {formatBatch(auctionId)} ]</span>
  }
  if (!link.url) {
    return (
      <span title={link.txHash} className="text-right text-brand-fg">
        [ {shortTxHash(link.txHash)} ]
      </span>
    )
  }
  return (
    <a
      href={link.url}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={`View settlement transaction ${link.txHash} on the block explorer`}
      title={link.txHash}
      className="text-right text-brand-fg underline decoration-brand-border underline-offset-4 transition-colors hover:decoration-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
    >
      [ {shortTxHash(link.txHash)} ]
    </a>
  )
}

export const FillHistoryRow = React.memo(FillHistoryRowImpl)
FillHistoryRow.displayName = 'FillHistoryRow'
