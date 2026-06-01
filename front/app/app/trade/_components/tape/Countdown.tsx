'use client'

import * as React from 'react'

import { secondsToNextAuction } from '../../_lib/tape/format'
import { StreamStatus } from './StreamStatus'
import type { StreamConnectionStatus } from '../../_hooks/tape/useAuctionStream'

export interface CountdownProps {
  latestAuctionUnixSeconds: bigint | null
  nowUnixSeconds: number
  /** Cadence of the auction tick. Mock store defaults to 5s. */
  intervalSeconds?: number
  /** Live-feed status; renders the LIVE/DELAYED badge at the bar's right edge. */
  status?: StreamConnectionStatus
}

const DEFAULT_INTERVAL_SECONDS = 5

export function Countdown({
  latestAuctionUnixSeconds,
  nowUnixSeconds,
  intervalSeconds = DEFAULT_INTERVAL_SECONDS,
  status,
}: CountdownProps): JSX.Element {
  const waiting = latestAuctionUnixSeconds === null

  const label = waiting
    ? '[ WAITING FOR FIRST AUCTION ]'
    : `[ NEXT AUCTION IN ${pad2(
        secondsToNextAuction(latestAuctionUnixSeconds, nowUnixSeconds, intervalSeconds)
      )} ]`

  return (
    <div
      aria-live="polite"
      aria-atomic="true"
      className={`relative flex h-9 items-center justify-center border-b border-brand-border bg-brand-bg px-4 font-mono text-label-lg uppercase tracking-label ${
        waiting ? 'text-brand-muted' : 'text-brand-accent'
      }`}
    >
      {label}
      {status ? (
        <span className="absolute right-3 top-1/2 -translate-y-1/2">
          <StreamStatus status={status} />
        </span>
      ) : null}
    </div>
  )
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}
