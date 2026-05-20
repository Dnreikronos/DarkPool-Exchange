'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import type { AuctionSummary } from '@/lib/sdk'

import { formatCount, formatRelativeTime } from './format'
import styles from './tape.module.css'

export interface TapeRowProps {
  auction: AuctionSummary
  /** Current Unix seconds; passed down so all rows share one tick source. */
  nowUnixSeconds: number
  /** Click / Enter / Space handler. */
  onSelect: (auctionId: string) => void
}

function TapeRowImpl({ auction, nowUnixSeconds, onSelect }: TapeRowProps): JSX.Element {
  const handleClick = React.useCallback(() => {
    onSelect(auction.auctionId)
  }, [auction.auctionId, onSelect])

  return (
    <li className="border-b border-brand-border">
      <button
        type="button"
        onClick={handleClick}
        className={`${styles.enter} grid w-full grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)_2rem] items-center gap-3 px-4 py-2 text-left font-mono text-body-sm tabular-nums text-brand-fg transition-colors hover:bg-brand-surface focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent`}
        aria-label={`Auction ${auction.auctionId}, clearing price ${auction.clearingPrice}`}
      >
        <span className="text-brand-muted">
          {formatRelativeTime(auction.timestampUnix, nowUnixSeconds)}
        </span>
        <NumericText
          value={auction.clearingPrice}
          kind="price"
          align="right"
          className="text-brand-fg"
        />
        <NumericText
          value={auction.matchedVolume}
          kind="size"
          align="right"
          className="text-brand-fg"
        />
        <span className="text-right text-brand-muted">{formatCount(auction.matchCount)}</span>
      </button>
    </li>
  )
}

export const TapeRow = React.memo(TapeRowImpl)
TapeRow.displayName = 'TapeRow'
