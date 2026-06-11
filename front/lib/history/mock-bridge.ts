// Mirrors the Phase-1 mock store into the persistent history (#101).
//
// The mock store is the simulated engine: placeOrder pushes into
// openOrders, runAuction moves matched orders into fillHistory, and
// cancelOrder drops them silently. This bridge watches those transitions
// and writes the same facts into Dexie, keyed by the connected trader —
// so the portfolio reads ONE source (the history db) in both mock and
// real modes, and mock-session history survives a refresh just like real
// history does.
//
// Transition mapping per store tick:
//   appeared in openOrders            → order record (status open)
//   new entry in fillHistory          → fill record + roll into the order
//   left openOrders with no fill      → cancelled
//
// Writes are serialized through a promise chain so a fill never races the
// insert of the order it consumes. `flush()` awaits the chain (tests).

import type { StoreApi } from 'zustand/vanilla'

import type { MockStore } from '@/lib/mock-store'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { HistoryDb } from './db'
import { fillToFillRecord, orderInfoToRecord } from './records'
import { applyFill, markOrderCancelled, recordSubmittedOrder } from './repo'

export interface MockHistoryMirrorDeps {
  db: HistoryDb
  /** Current trader key (normalizeTraderId form), or null when disconnected. */
  getTrader: () => string | null
}

export interface MockHistoryMirror {
  stop(): void
  /** Resolves once every write queued so far has settled. */
  flush(): Promise<void>
}

export function startMockHistoryMirror(
  store: StoreApi<MockStore>,
  deps: MockHistoryMirrorDeps
): MockHistoryMirror {
  let prevOpen = new Map<string, OrderInfo>()
  let prevFillIds = new Set<string>()
  let queue: Promise<void> = Promise.resolve()

  const enqueue = (work: () => Promise<void>) => {
    // Swallow individual write failures: history persistence is
    // best-effort and must never break the trading session.
    queue = queue.then(work).catch(() => undefined)
  }

  const sync = () => {
    const state = store.getState()
    const nextOpen = new Map(state.openOrders.map((o) => [o.id, o]))
    const nextFillIds = new Set(state.fillHistory.map((f) => f.fillId))
    const newFills = state.fillHistory.filter((f) => !prevFillIds.has(f.fillId))
    const appeared = state.openOrders.filter((o) => !prevOpen.has(o.id))
    const departed = [...prevOpen.values()].filter((o) => !nextOpen.has(o.id))
    prevOpen = nextOpen
    prevFillIds = nextFillIds

    const trader = deps.getTrader()
    if (trader === null) return

    const filledIds = new Set(newFills.map((f) => f.orderId))
    enqueue(async () => {
      for (const order of appeared) {
        await recordSubmittedOrder(deps.db, orderInfoToRecord(order, trader))
      }
      for (const fill of newFills) {
        await applyFill(deps.db, fillToFillRecord(fill, trader))
      }
      for (const order of departed) {
        if (!filledIds.has(order.id)) await markOrderCancelled(deps.db, order.id)
      }
    })
  }

  // Capture whatever is already resting at start (refresh mid-session).
  const initial = store.getState()
  prevFillIds = new Set(initial.fillHistory.map((f) => f.fillId))
  const trader = deps.getTrader()
  if (trader !== null) {
    prevOpen = new Map(initial.openOrders.map((o) => [o.id, o]))
    enqueue(async () => {
      for (const order of initial.openOrders) {
        await recordSubmittedOrder(deps.db, orderInfoToRecord(order, trader))
      }
    })
  }

  const unsubscribe = store.subscribe(sync)

  return {
    stop: unsubscribe,
    flush: () => queue,
  }
}
