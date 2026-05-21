'use client'

import * as React from 'react'

import { statusLabel } from '../../_lib/my-orders/format'
import type { MyOrderStatus } from '../../_lib/my-orders/types'

export interface StatusPillProps {
  status: MyOrderStatus
}

// Mirrors `status-pill-live` / `status-pill-offline` from DESIGN.md:
// a 6×6 square (`borderRadius: 0` is mandatory) paired with a bracketed
// mono label. Open blinks at 1 Hz; the other states are static so the
// trader's eye lands on whatever just changed.
//
// Per DESIGN-INSPIRATIONS §"Accent budget per view", the /app/trade
// surface gives the lime accent to the auction countdown — not here.
// The OPEN affordance reads as "live" through shape + motion (square,
// blinking) rather than colour.
const SQUARE_BASE = 'inline-block h-1.5 w-1.5 align-middle [border-radius:0px]'

const SQUARE_CLASS: Record<MyOrderStatus, string> = {
  open: `${SQUARE_BASE} bg-brand-fg animate-blink motion-reduce:animate-none`,
  filled: `${SQUARE_BASE} bg-brand-fg`,
  cancelled: `${SQUARE_BASE} bg-brand-muted`,
}

const TEXT_CLASS: Record<MyOrderStatus, string> = {
  open: 'text-brand-fg',
  filled: 'text-brand-fg',
  cancelled: 'text-brand-muted',
}

export function StatusPill({ status }: StatusPillProps): JSX.Element {
  return (
    <span className="inline-flex items-center gap-2 font-mono text-body-sm">
      <span aria-hidden="true" className={SQUARE_CLASS[status]} />
      <span className={`uppercase tracking-label ${TEXT_CLASS[status]}`}>
        {statusLabel(status)}
      </span>
    </span>
  )
}
