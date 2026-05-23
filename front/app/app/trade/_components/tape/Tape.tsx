'use client'

import * as React from 'react'

import type { AuctionSummary } from '@/lib/sdk'

import { Countdown } from './Countdown'
import { TapeDrawer } from './TapeDrawer'
import { TapeRow } from './TapeRow'
import { TapeEmpty } from './states'
import { useAuctionHistory } from '../../_hooks/tape/useAuctionHistory'
import { useNow } from '../../_hooks/tape/useNow'

export interface TapeProps {
  limit?: number
}

export function Tape({ limit }: TapeProps = {}): JSX.Element {
  const auctions = useAuctionHistory({ limit })
  const nowSeconds = useNow()
  const [selectedId, setSelectedId] = React.useState<string | null>(null)

  const selected = React.useMemo<AuctionSummary | null>(() => {
    if (selectedId === null) return null
    return auctions.find((a) => a.auctionId === selectedId) ?? null
  }, [auctions, selectedId])

  const latestUnix: bigint | null = auctions.length > 0 ? auctions[0].timestampUnix : null

  return (
    <div className="flex h-full min-h-[200px] flex-col">
      <Countdown latestAuctionUnixSeconds={latestUnix} nowUnixSeconds={nowSeconds} />
      <TableHeader />
      {auctions.length === 0 ? (
        <TapeEmpty />
      ) : (
        <ol aria-live="polite" aria-atomic="false" className="flex-1 overflow-y-auto">
          {auctions.map((a) => (
            <TapeRow
              key={a.auctionId}
              auction={a}
              nowUnixSeconds={nowSeconds}
              onSelect={setSelectedId}
            />
          ))}
        </ol>
      )}
      <TapeDrawer auction={selected} onClose={() => setSelectedId(null)} />
    </div>
  )
}

function TableHeader(): JSX.Element {
  return (
    <div className="grid grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)_2rem] gap-3 border-b border-brand-border bg-brand-surface px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
      <span>TIME</span>
      <span className="text-right">PRICE</span>
      <span className="text-right">VOLUME</span>
      <span className="text-right">MATCH</span>
    </div>
  )
}
