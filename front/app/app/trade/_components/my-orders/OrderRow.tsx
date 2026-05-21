'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'

import { formatSubmittedAt, sideLabel } from '../../_lib/my-orders/format'
import type { MyOrderRow } from '../../_lib/my-orders/types'
import { StatusPill } from './StatusPill'

export interface OrderRowProps {
  row: MyOrderRow
  /** Invoked when the trader clicks Cancel on an open row. */
  onCancel: (orderId: string) => void
}

// Six-column layout matching DESIGN-INSPIRATIONS §My orders:
// time · side · price · size · status · cancel. Numerics align right
// with tabular figures so they read like a Bloomberg blotter.
const COLS = 'grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_minmax(0,1fr)_6rem_5.5rem]'

export function OrderRow({ row, onCancel }: OrderRowProps): JSX.Element {
  const { order, status } = row
  const cancellable = status === 'open'
  const dimmed = status !== 'open'

  return (
    <div
      role="row"
      data-testid="my-order-row"
      data-status={status}
      className={`grid ${COLS} items-center gap-3 border-b border-brand-border px-4 py-3 font-mono text-body-sm tabular-nums ${
        dimmed ? 'opacity-60' : ''
      }`}
    >
      <span className="uppercase tracking-label text-brand-muted">
        {formatSubmittedAt(order.submittedAtUnix)}
      </span>
      <span className="uppercase tracking-label text-brand-fg">{sideLabel(order.side)}</span>
      <NumericText value={order.price} kind="price" align="right" className="text-brand-fg" />
      <NumericText
        value={order.remainingSize}
        kind="size"
        align="right"
        className="text-brand-fg"
      />
      <span className="flex justify-start">
        <StatusPill status={status} />
      </span>
      <div className="flex justify-end">
        <button
          type="button"
          disabled={!cancellable}
          onClick={cancellable ? () => onCancel(order.id) : undefined}
          aria-label={`Cancel order ${order.id}`}
          className="inline-flex h-9 min-w-[3.5rem] items-center justify-end font-mono text-label-md uppercase tracking-labelWide text-brand-muted transition-colors hover:text-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:text-brand-muted"
        >
          [ CANCEL ]
        </button>
      </div>
    </div>
  )
}
