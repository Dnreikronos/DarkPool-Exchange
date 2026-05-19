'use client'

import * as React from 'react'

import type { Fill } from '../../../lib/mock-store'

import { ExportCsvButton } from './ExportCsvButton'
import { FillHistoryRow } from './FillHistoryRow'

export interface FillHistoryTableProps {
  fills: readonly Fill[]
}

export function FillHistoryTable({ fills }: FillHistoryTableProps): JSX.Element {
  return (
    <section
      aria-label="Fill history"
      className="flex min-h-[280px] flex-col border border-brand-border bg-brand-surface"
    >
      <TableTitleBar count={fills.length} fills={fills} />
      <TableHeader />
      {fills.length === 0 ? (
        <EmptyState />
      ) : (
        <ol className="flex-1 overflow-y-auto">
          {fills.map((f) => (
            <FillHistoryRow key={f.fillId} fill={f} />
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
  return (
    <div
      className="grid grid-cols-[10rem_4rem_minmax(0,1fr)_minmax(0,1fr)_minmax(0,9rem)] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
      role="row"
    >
      <span>TIME</span>
      <span>SIDE</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">SIZE</span>
      <span className="text-right">BATCH</span>
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex flex-1 items-center justify-center px-4 py-12">
      <p
        role="status"
        className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
      >
        [ NO FILLS YET — PLACE AN ORDER ON /APP/TRADE ]
      </p>
    </div>
  )
}
