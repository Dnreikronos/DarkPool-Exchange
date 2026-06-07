// Session-scoped store of BatchSettled events observed on-chain (#100).
// A single watcher (SettlementWatcher in the /app layout) appends here;
// panels on any route correlate their own auctions/fills against the
// list via `correlateSettlements`. Mirrors lib/mock-store.ts: zustand
// vanilla + `Symbol.for`-keyed globalThis slot so the singleton survives
// Next.js HMR in dev.

import { useSyncExternalStore } from 'react'
import { createStore, type StoreApi } from 'zustand/vanilla'

import type { SettlementEvent } from './correlate'

/**
 * Retention cap. Events are only useful for ±30s correlation against
 * auctions the UI still shows (tape caps at 200), so a few hundred is
 * plenty for a session.
 */
export const SETTLEMENT_EVENTS_CAP = 256

export interface SettlementStoreState {
  /** Newest first, unique by batchId, capped at SETTLEMENT_EVENTS_CAP. */
  events: SettlementEvent[]
  /** Append observed events; duplicates (by batchId) are ignored. */
  addEvents(incoming: readonly SettlementEvent[]): void
}

export type SettlementStore = SettlementStoreState

export function createSettlementStore(): StoreApi<SettlementStore> {
  return createStore<SettlementStore>((set, get) => ({
    events: [],
    addEvents(incoming) {
      const { events } = get()
      const seen = new Set(events.map((e) => e.batchId))
      const fresh = incoming.filter((e) => {
        if (seen.has(e.batchId)) return false
        seen.add(e.batchId)
        return true
      })
      if (fresh.length === 0) return
      const merged = [...fresh, ...events]
      merged.sort((a, b) =>
        a.timestampUnix < b.timestampUnix ? 1 : a.timestampUnix > b.timestampUnix ? -1 : 0
      )
      set({ events: merged.slice(0, SETTLEMENT_EVENTS_CAP) })
    },
  }))
}

// ─── Runtime singleton ─────────────────────────────────────────────────────

const SETTLEMENT_STORE_KEY = Symbol.for('darkpool.settlementStore.v1')

type GlobalSlot = Record<symbol, unknown>
const globalSlot = globalThis as unknown as GlobalSlot

const existing = globalSlot[SETTLEMENT_STORE_KEY] as StoreApi<SettlementStore> | undefined
export const settlementStore: StoreApi<SettlementStore> = existing ?? createSettlementStore()
globalSlot[SETTLEMENT_STORE_KEY] = settlementStore

// ─── React binding ─────────────────────────────────────────────────────────

/** Subscribe to the observed settlement events (newest first). */
export function useSettlementEvents(): readonly SettlementEvent[] {
  return useSyncExternalStore(
    settlementStore.subscribe,
    () => settlementStore.getState().events,
    () => settlementStore.getState().events
  )
}
