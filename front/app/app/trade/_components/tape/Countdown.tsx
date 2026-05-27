'use client'

import * as React from 'react'

import { secondsToNextAuction } from '../../_lib/tape/format'

export interface CountdownProps {
  latestAuctionUnixSeconds: bigint | null
  nowUnixSeconds: number
  /** Cadence of the auction tick. Mock store defaults to 5s. */
  intervalSeconds?: number
}

const DEFAULT_INTERVAL_SECONDS = 5

export function Countdown({
  latestAuctionUnixSeconds,
  nowUnixSeconds,
  intervalSeconds = DEFAULT_INTERVAL_SECONDS,
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
      className={`flex h-9 items-center justify-center border-b border-brand-border bg-brand-bg px-4 font-mono text-label-lg uppercase tracking-label ${
        waiting ? 'text-brand-muted' : 'text-brand-accent'
      }`}
    >
      {label}
    </div>
  )
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}
