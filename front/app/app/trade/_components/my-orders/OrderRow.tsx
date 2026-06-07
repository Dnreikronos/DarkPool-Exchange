'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import { shortTxHash, type SettlementLink } from '@/lib/settlement'

import { formatSubmittedAt, sideLabel } from '../../_lib/my-orders/format'
import type { MyOrderRow } from '../../_lib/my-orders/types'
import { StatusPill } from './StatusPill'

export interface OrderRowProps {
  row: MyOrderRow
  /**
   * On-chain settlement correlated to the fill that consumed this order
   * (#100). Only rendered on `filled` rows, where it takes over the
   * action column (the cancel affordance is moot there).
   */
  link?: SettlementLink | null
  /** Invoked when the trader clicks Cancel on an open row. */
  onCancel: (orderId: string) => void
}

// Six-column layout matching DESIGN-INSPIRATIONS §My orders:
// time · side · price · size · status · action. Numerics align right
// with tabular figures so they read like a Bloomberg blotter. The
// action track fits the wider of `[ CANCEL ]` and a settlement hash
// like `[ 0xdead…beef ]` (#100). Keep in sync with ColumnHeader in
// MyOrdersPanel.tsx.
const COLS = 'grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_minmax(0,1fr)_6rem_7.5rem]'

export function OrderRow({ row, link = null, onCancel }: OrderRowProps): JSX.Element {
  const { order, status } = row
  const cancellable = status === 'open'
  const settled = status === 'filled' ? link : null

  return (
    <div
      role="row"
      data-testid="my-order-row"
      data-status={status}
      className={`grid ${COLS} items-center gap-3 border-b border-brand-border px-4 py-3 font-mono text-body-sm tabular-nums ${
        status !== 'open' ? 'opacity-60' : ''
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
        {settled ? (
          <SettlementCell link={settled} />
        ) : (
          <button
            type="button"
            disabled={!cancellable}
            onClick={cancellable ? () => onCancel(order.id) : undefined}
            aria-label={`Cancel order ${order.id}`}
            className="inline-flex h-9 min-w-[3.5rem] items-center justify-end font-mono text-label-md uppercase tracking-labelWide text-brand-muted transition-colors hover:text-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:text-brand-muted"
          >
            [ CANCEL ]
          </button>
        )}
      </div>
    </div>
  )
}

/** Settlement receipt occupying the action column of a filled row. */
function SettlementCell({ link }: { link: SettlementLink }): JSX.Element {
  if (!link.url) {
    return (
      <span
        title={link.txHash}
        className="inline-flex h-9 items-center justify-end font-mono text-label-md tracking-label text-brand-fg tabular-nums"
      >
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
      className="inline-flex h-9 items-center justify-end font-mono text-label-md tracking-label text-brand-fg tabular-nums underline decoration-brand-border underline-offset-4 transition-colors hover:decoration-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
    >
      [ {shortTxHash(link.txHash)} ]
    </a>
  )
}
