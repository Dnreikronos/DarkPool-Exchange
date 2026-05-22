'use client'

// Composition root for the My Orders panel.
//
// Reads (rows, cancel) from useMyOrders, fires a toast on cancel, and
// renders three states:
//   - Disconnected (mock wallet not connected) → empty hint.
//   - Connected + no rows → "[ NO ORDERS YET ]".
//   - Connected + rows → header row + ordered row list.
//
// The hook is injectable so the test surface can drive the panel from
// a fresh mock-store without touching the runtime singleton.

import * as React from 'react'

import { useToast } from '@/components/ui/use-toast'
import { useWallet } from '@/lib/wallet/hooks'

import {
  useMyOrders,
  type UseMyOrdersOptions,
  type UseMyOrdersReturn,
} from '../../_hooks/my-orders/useMyOrders'
import { OrderRow } from './OrderRow'

export interface MyOrdersPanelProps {
  /**
   * Override the hook used to source rows. Tests pass a controlled
   * implementation; production code lets the default singleton hook run.
   */
  useOrders?: (options?: UseMyOrdersOptions) => UseMyOrdersReturn
}

export function MyOrdersPanel({ useOrders = useMyOrders }: MyOrdersPanelProps = {}): JSX.Element {
  const { isConnected } = useWallet()
  const { rows, cancel } = useOrders()
  const { toast } = useToast()
  const headerId = React.useId()

  const handleCancel = React.useCallback(
    (orderId: string) => {
      const ok = cancel(orderId)
      if (!ok) return
      toast({ title: 'Order cancelled', description: 'Removed from the open book.' })
    },
    [cancel, toast]
  )

  return (
    <section
      aria-labelledby={headerId}
      className="flex h-full min-h-[160px] flex-col border border-brand-border bg-brand-surface"
    >
      <Header id={headerId} />
      {!isConnected ? (
        <EmptyState label="[ CONNECT WALLET ]" />
      ) : rows.length === 0 ? (
        <EmptyState label="[ NO ORDERS YET ]" />
      ) : (
        <div className="flex flex-1 flex-col overflow-y-auto">
          <ColumnHeader />
          <div role="rowgroup">
            {rows.map((row) => (
              <OrderRow key={row.order.id} row={row} onCancel={handleCancel} />
            ))}
          </div>
        </div>
      )}
    </section>
  )
}

function Header({ id }: { id: string }): JSX.Element {
  return (
    <header className="flex h-9 items-center border-b border-brand-border px-4">
      <span
        id={id}
        className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
      >
        [ MY ORDERS ]
      </span>
    </header>
  )
}

function ColumnHeader(): JSX.Element {
  return (
    <div
      role="row"
      className="grid grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_minmax(0,1fr)_6rem_5.5rem] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
    >
      <span>TIME</span>
      <span>SIDE</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">SIZE</span>
      <span>STATUS</span>
      <span className="text-right" aria-hidden="true">
        {/* cancel column header is intentionally blank — action, not data */}
      </span>
    </div>
  )
}

function EmptyState({ label }: { label: string }): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <p
        role="status"
        className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
      >
        {label}
      </p>
    </div>
  )
}
