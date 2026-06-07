'use client'

import * as React from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

import type { AuctionSummary } from '@/lib/sdk'

import { Countdown } from './Countdown'
import { TapeDrawer } from './TapeDrawer'
import { TapeRow } from './TapeRow'
import { TapeEmpty } from './states'
import { useAuctionFeed } from '../../_hooks/tape/useAuctionFeed'
import { useNow } from '../../_hooks/tape/useNow'

export interface TapeProps {
  limit?: number
  /** Override polling cadence for Storybook / tests. */
  refetchIntervalMs?: number
}

/**
 * Root tape component.
 *
 * Ships its own `QueryClientProvider` mirroring `OrderBook`: the trading
 * shell hasn't hoisted a shared client yet, so each panel scopes one.
 * Consumers that already wrap children with a `QueryClientProvider`
 * should render {@link TapeContent} directly to share the cache.
 */
export function Tape(props: TapeProps = {}): JSX.Element {
  return (
    <QueryClientProvider client={getScopedClient()}>
      <TapeContent {...props} />
    </QueryClientProvider>
  )
}

export function TapeContent({ limit, refetchIntervalMs }: TapeProps = {}): JSX.Element {
  const { auctions, status } = useAuctionFeed({ limit, refetchIntervalMs })
  const nowSeconds = useNow()
  const [selectedId, setSelectedId] = React.useState<string | null>(null)

  const selected = React.useMemo<AuctionSummary | null>(() => {
    if (selectedId === null) return null
    return auctions.find((a) => a.auctionId === selectedId) ?? null
  }, [auctions, selectedId])

  const latestUnix: bigint | null = auctions.length > 0 ? auctions[0].timestampUnix : null

  return (
    <div className="flex h-full min-h-[200px] flex-col">
      <Countdown
        latestAuctionUnixSeconds={latestUnix}
        nowUnixSeconds={nowSeconds}
        status={status}
      />
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

let scopedClient: QueryClient | null = null
function getScopedClient(): QueryClient {
  if (scopedClient) return scopedClient
  scopedClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
      },
    },
  })
  return scopedClient
}
