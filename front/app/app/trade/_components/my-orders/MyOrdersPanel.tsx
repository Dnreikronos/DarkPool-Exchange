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
import { useMockStore, type Fill, type MockStoreState } from '@/lib/mock-store'
import {
  correlateSettlements,
  settlementLink,
  useSettlementEvents,
  type SettlementLink,
} from '@/lib/settlement'
import { useWallet } from '@/lib/wallet/hooks'

import {
  useMyOrders,
  type UseMyOrdersOptions,
  type UseMyOrdersReturn,
} from '../../_hooks/my-orders/useMyOrders'
import { OrderRow } from './OrderRow'
import { MyOrdersEmpty } from './states'

export interface MyOrdersPanelProps {
  /**
   * Override the hook used to source rows. Tests pass a controlled
   * implementation; production code lets the default singleton hook run.
   */
  useOrders?: (options?: UseMyOrdersOptions) => UseMyOrdersReturn
}

const selectFillHistory = (state: MockStoreState): readonly Fill[] => state.fillHistory

/**
 * orderId → settlement link (#100). A filled order is tied to its
 * auction through the fill history, and the auction to a BatchSettled
 * event by timestamp correlation. Newest-first fill order means a
 * partially-filled order surfaces its most recent settlement.
 */
function useOrderSettlementLinks(): ReadonlyMap<string, SettlementLink> {
  const fills = useMockStore(selectFillHistory)
  const events = useSettlementEvents()
  return React.useMemo(() => {
    const byAuction = correlateSettlements(fills, events)
    const byOrder = new Map<string, SettlementLink>()
    for (const fill of fills) {
      if (byOrder.has(fill.orderId)) continue
      const link = settlementLink(byAuction.get(fill.auctionId))
      if (link) byOrder.set(fill.orderId, link)
    }
    return byOrder
  }, [fills, events])
}

export function MyOrdersPanel({ useOrders = useMyOrders }: MyOrdersPanelProps = {}): JSX.Element {
  const { isConnected } = useWallet()
  const { rows, cancel } = useOrders()
  const links = useOrderSettlementLinks()
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
        <MyOrdersEmpty disconnected />
      ) : rows.length === 0 ? (
        <MyOrdersEmpty />
      ) : (
        <div className="flex flex-1 flex-col overflow-y-auto">
          <ColumnHeader />
          <div role="rowgroup">
            {rows.map((row) => (
              <OrderRow
                key={row.order.id}
                row={row}
                link={links.get(row.order.id) ?? null}
                onCancel={handleCancel}
              />
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
      className="grid grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_minmax(0,1fr)_6rem_7.5rem] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
    >
      <span>TIME</span>
      <span>SIDE</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">SIZE</span>
      <span>STATUS</span>
      <span className="text-right">
        {/* visually blank: the column holds the cancel action, or the
            settlement tx once a filled row links (#100) */}
        <span className="sr-only">Action / settlement transaction</span>
      </span>
    </div>
  )
}
