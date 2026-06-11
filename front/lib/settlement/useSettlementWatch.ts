'use client'

// Chain-side half of the settlement linkage (#100): subscribes to
// DarkPool's BatchSettled(batchId, timestamp) and appends each
// occurrence (plus its tx hash) to the settlement store. Mounted once
// per session by SettlementWatcher in the /app layout so events
// observed while trading are still correlatable on /app/portfolio.
//
// Note: only the Groth16 path emits BatchSettled. The HyperNova IVC
// path settles via AuctionSettled(auctionId, batchId) — extend the
// watcher when that path goes live (direct id linkage, no window).

import { useCallback } from 'react'
import { useWatchContractEvent } from 'wagmi'
import type { StoreApi } from 'zustand/vanilla'

import { config } from '@/lib/config'
import { darkPoolAbi } from '@/lib/contracts/generated'

import { settlementEventsFromLogs, type BatchSettledLog } from './events'
import { settlementStore, type SettlementStore } from './store'

export function useSettlementWatch(store: StoreApi<SettlementStore> = settlementStore): void {
  const addrs = config.contracts
  // Stable identity so the watcher doesn't tear down/re-register on
  // re-renders (same reasoning as useChainBalances).
  const onLogs = useCallback(
    (logs: readonly unknown[]) => {
      store.getState().addEvents(settlementEventsFromLogs(logs as readonly BatchSettledLog[]))
    },
    [store]
  )
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'BatchSettled',
    // Dormant under mocks / missing contract config — same gating as
    // useChainBalances. BatchSettled has no indexed trader, so there is
    // no per-wallet filter; the subscription is session-scoped instead.
    enabled: !config.useMocks && Boolean(addrs),
    onLogs,
  })
}
