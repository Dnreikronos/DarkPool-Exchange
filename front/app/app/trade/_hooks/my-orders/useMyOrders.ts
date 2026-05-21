'use client'

// React shell over the pure afterlife reducer in _lib/my-orders/afterlife.ts.
//
// The hook fuses two state sources:
//   1. mockStore.openOrders, subscribed via useSyncExternalStore.
//   2. A local "afterlife" map that keeps cancelled/filled rows on screen
//      for `ttlMs` after the underlying store has dropped them.
//
// On every change to openOrders the hook diffs against the previous
// snapshot and writes a `filled` tombstone for any id that left without
// a prior `cancelled` marker. `cancel()` writes the tombstone *before*
// calling `store.cancelOrder` so the order is captured at the source
// rather than reconstructed after the fact.
//
// All the algebra lives in `_lib/my-orders/afterlife.ts`; this file is
// glue.

import { useCallback, useEffect, useMemo, useReducer, useRef, useSyncExternalStore } from 'react'
import type { StoreApi } from 'zustand/vanilla'

import { mockStore as defaultMockStore } from '@/lib/mock-store'
import type { MockStore } from '@/lib/mock-store'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  appendFilled,
  composeRows,
  DEFAULT_AFTERLIFE_TTL_MS,
  emptyAfterlifeState,
  markCancelled,
  pruneAfterlife,
  userPriceLevels,
  type AfterlifeState,
} from '../../_lib/my-orders/afterlife'
import type { MyOrderRow } from '../../_lib/my-orders/types'

type Action =
  | {
      type: 'SYNC'
      prev: ReadonlyMap<string, OrderInfo>
      next: ReadonlyMap<string, OrderInfo>
      nowMs: number
      ttlMs: number
    }
  | { type: 'CANCEL'; order: OrderInfo; nowMs: number }
  | { type: 'PRUNE'; nowMs: number; ttlMs: number }

function reducer(state: AfterlifeState, action: Action): AfterlifeState {
  switch (action.type) {
    case 'SYNC': {
      const withFills = appendFilled(state, {
        prevOpenOrders: action.prev,
        nextOpenOrders: action.next,
        nowMs: action.nowMs,
      })
      return pruneAfterlife(withFills, action.nowMs, action.ttlMs)
    }
    case 'CANCEL':
      return markCancelled(state, action.order, action.nowMs)
    case 'PRUNE':
      return pruneAfterlife(state, action.nowMs, action.ttlMs)
  }
}

// Hoisted so the default identity is stable across renders and we don't
// thrash effect dependencies.
const wallClock = (): number => Date.now()

export interface UseMyOrdersOptions {
  /** Injectable store handle. Defaults to the runtime singleton. */
  store?: StoreApi<MockStore>
  /** Afterlife TTL in ms. Defaults to 5000 (matches the design spec). */
  ttlMs?: number
  /** Injectable clock so tests can pin removal timestamps. */
  now?: () => number
}

export interface UseMyOrdersReturn {
  rows: MyOrderRow[]
  /** Distinct prices the user has open orders at — used by the orderbook to mark the level. */
  userPrices: Set<string>
  /** Trigger a cancel. Returns true iff the order was found and removed. */
  cancel: (orderId: string) => boolean
}

export function useMyOrders(options: UseMyOrdersOptions = {}): UseMyOrdersReturn {
  const store = options.store ?? defaultMockStore
  const ttlMs = options.ttlMs ?? DEFAULT_AFTERLIFE_TTL_MS
  const now = options.now ?? wallClock

  const subscribe = useCallback((listener: () => void) => store.subscribe(listener), [store])
  const getSnapshot = useCallback(() => store.getState().openOrders, [store])
  const openOrders = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)

  const [afterlife, dispatch] = useReducer(reducer, undefined, emptyAfterlifeState)

  // Snapshot held across renders so the SYNC dispatch can diff prev→next.
  const prevRef = useRef<ReadonlyMap<string, OrderInfo>>(new Map(openOrders.map((o) => [o.id, o])))

  useEffect(() => {
    const next = new Map(openOrders.map((o) => [o.id, o]))
    if (sameIdSet(prevRef.current, next)) return
    dispatch({ type: 'SYNC', prev: prevRef.current, next, nowMs: now(), ttlMs })
    prevRef.current = next
  }, [openOrders, now, ttlMs])

  // Heartbeat prune so cancelled/filled rows fade out even when the
  // mock store is otherwise quiet. The interval is short enough that
  // expiry feels punctual without being noisy.
  useEffect(() => {
    const id = setInterval(() => dispatch({ type: 'PRUNE', nowMs: now(), ttlMs }), 1000)
    return () => clearInterval(id)
  }, [now, ttlMs])

  const cancel = useCallback(
    (orderId: string): boolean => {
      const state = store.getState()
      const order = state.openOrders.find((o) => o.id === orderId)
      if (!order) return false
      dispatch({ type: 'CANCEL', order, nowMs: now() })
      return state.cancelOrder(orderId)
    },
    [store, now]
  )

  const rows = useMemo(
    () => composeRows(openOrders, afterlife.afterlife),
    [openOrders, afterlife.afterlife]
  )
  const userPrices = useMemo(() => userPriceLevels(openOrders), [openOrders])

  return { rows, userPrices, cancel }
}

function sameIdSet(a: ReadonlyMap<string, OrderInfo>, b: ReadonlyMap<string, OrderInfo>): boolean {
  if (a.size !== b.size) return false
  for (const id of a.keys()) if (!b.has(id)) return false
  return true
}
