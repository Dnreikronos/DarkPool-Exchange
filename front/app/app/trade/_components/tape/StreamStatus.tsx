import * as React from 'react'

import type { StreamConnectionStatus } from '../../_hooks/tape/useAuctionStream'

// Mirrors `my-orders/StatusPill` and DESIGN.md `status-pill-*`: a 6×6 square
// (h-1.5 w-1.5, borderRadius 0) paired with a mono body-sm label. Per
// DESIGN-INSPIRATIONS §"Accent budget per view", the /app/trade surface
// already spends its single lime accent on the auction Countdown — so "live"
// reads through shape + motion (white square, 1 Hz blink), NOT colour.
// "Delayed" is a static muted square.
const SQUARE_BASE = 'inline-block h-1.5 w-1.5 shrink-0 align-middle [border-radius:0px]'

const PILL: Record<StreamConnectionStatus, string> = {
  live: `${SQUARE_BASE} bg-brand-fg animate-blink motion-reduce:animate-none`,
  connecting: `${SQUARE_BASE} bg-brand-muted`,
  degraded: `${SQUARE_BASE} bg-brand-muted`,
}

const LABEL: Record<StreamConnectionStatus, string> = {
  live: 'LIVE',
  connecting: 'DELAYED',
  degraded: 'DELAYED',
}

const LABEL_COLOR: Record<StreamConnectionStatus, string> = {
  live: 'text-brand-fg',
  connecting: 'text-brand-muted',
  degraded: 'text-brand-muted',
}

export interface StreamStatusProps {
  status: StreamConnectionStatus
}

export function StreamStatus({ status }: StreamStatusProps): JSX.Element {
  return (
    <span className="inline-flex items-center gap-2 font-mono text-body-sm" aria-live="polite">
      <span className={PILL[status]} aria-hidden="true" />
      <span className={`uppercase tracking-label ${LABEL_COLOR[status]}`}>{LABEL[status]}</span>
    </span>
  )
}
