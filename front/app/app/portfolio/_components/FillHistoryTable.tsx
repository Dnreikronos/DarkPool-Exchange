'use client'

import * as React from 'react'

import type { Fill } from '@/lib/mock-store'
import {
  correlateSettlements,
  settlementLink,
  useSettlementEvents,
  type SettlementLink,
} from '@/lib/settlement'

import { ExportCsvButton } from './ExportCsvButton'
import { FillHistoryRow } from './FillHistoryRow'
import { FillHistoryEmpty } from './states'

export interface FillHistoryTableProps {
  fills: readonly Fill[]
}

/**
 * fillId → settlement link, derived from BatchSettled events observed
 * this session (#100). Memoised so row links keep a stable identity and
 * `React.memo` on FillHistoryRow stays effective across re-renders.
 */
function useFillSettlementLinks(fills: readonly Fill[]): ReadonlyMap<string, SettlementLink> {
  const events = useSettlementEvents()
  return React.useMemo(() => {
    const byAuction = correlateSettlements(fills, events)
    const byFill = new Map<string, SettlementLink>()
    for (const fill of fills) {
      const link = settlementLink(byAuction.get(fill.auctionId))
      if (link) byFill.set(fill.fillId, link)
    }
    return byFill
  }, [fills, events])
}

export function FillHistoryTable({ fills }: FillHistoryTableProps): JSX.Element {
  const links = useFillSettlementLinks(fills)
  return (
    <section
      aria-label="Fill history"
      className="flex min-h-[280px] flex-col border border-brand-border bg-brand-surface"
    >
      <TableTitleBar count={fills.length} fills={fills} />
      <TableHeader />
      {fills.length === 0 ? (
        <FillHistoryEmpty />
      ) : (
        <ol className="flex-1 overflow-y-auto">
          {fills.map((f) => (
            <FillHistoryRow key={f.fillId} fill={f} link={links.get(f.fillId) ?? null} />
          ))}
        </ol>
      )}
    </section>
  )
}

function TableTitleBar({ count, fills }: { count: number; fills: readonly Fill[] }) {
  return (
    <div className="flex h-9 items-center justify-between border-b border-brand-border px-4">
      <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        [ FILL HISTORY · {count.toString().padStart(2, '0')} ]
      </span>
      <ExportCsvButton fills={fills} />
    </div>
  )
}

function TableHeader() {
  // Visual column guide only — not an ARIA table. An orphan role="row"
  // (no table/rowgroup ancestor) is an axe violation; each list row below
  // carries a complete aria-label instead (#80).
  return (
    <div
      aria-hidden="true"
      className="grid grid-cols-[10rem_4rem_minmax(0,1fr)_minmax(0,1fr)_minmax(0,9rem)] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
    >
      <span>TIME</span>
      <span>SIDE</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">SIZE</span>
      <span className="text-right">BATCH</span>
    </div>
  )
}
