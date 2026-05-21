// "Afterlife" bookkeeping: the My Orders panel needs to surface orders
// that have *just* left the engine's openOrders so the trader sees their
// cancel/fill happen. The mock-store doesn't carry that information —
// once an order is gone, it's gone — so this module diffs successive
// openOrders snapshots and keeps a short-lived tombstone for each
// departed id.
//
// The semantics are:
//   • A user-initiated cancel calls `markCancelled` *before* the store
//     drops the order, so the snapshot is captured at the source.
//   • Everything else that leaves openOrders is attributed to a fill
//     by the auction tick. `appendFilled` runs after every store
//     change and writes a `filled` tombstone for ids that left without
//     a pre-existing `cancelled` marker.
//   • `pruneAfterlife` drops tombstones once their age exceeds the TTL.
//
// All functions are pure; the hook layer owns the mutable refs and
// timers that drive them.

import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { MyOrderRow, MyOrderStatus } from './types'

export type AfterlifeStatus = Extract<MyOrderStatus, 'filled' | 'cancelled'>

export interface AfterlifeEntry {
  order: OrderInfo
  status: AfterlifeStatus
  /** ms-precision timestamp the entry first entered its current status. */
  removedAtMs: number
}

export interface AfterlifeState {
  afterlife: ReadonlyMap<string, AfterlifeEntry>
}

/** TTL the panel uses when no override is supplied — matches DESIGN-INSPIRATIONS §My orders. */
export const DEFAULT_AFTERLIFE_TTL_MS = 5000

export function emptyAfterlifeState(): AfterlifeState {
  return { afterlife: new Map() }
}

export interface AppendFilledInput {
  prevOpenOrders: ReadonlyMap<string, OrderInfo>
  nextOpenOrders: ReadonlyMap<string, OrderInfo>
  nowMs: number
}

export function appendFilled(state: AfterlifeState, input: AppendFilledInput): AfterlifeState {
  let next: Map<string, AfterlifeEntry> | null = null
  for (const [id, prevOrder] of input.prevOpenOrders) {
    if (input.nextOpenOrders.has(id)) continue
    const existing = state.afterlife.get(id)
    if (existing && existing.status === 'cancelled') continue
    if (existing && existing.status === 'filled') continue
    if (!next) next = new Map(state.afterlife)
    next.set(id, { order: prevOrder, status: 'filled', removedAtMs: input.nowMs })
  }
  return next ? { afterlife: next } : state
}

export function markCancelled(
  state: AfterlifeState,
  order: OrderInfo,
  nowMs: number
): AfterlifeState {
  const next = new Map(state.afterlife)
  next.set(order.id, { order, status: 'cancelled', removedAtMs: nowMs })
  return { afterlife: next }
}

export function pruneAfterlife(
  state: AfterlifeState,
  nowMs: number,
  ttlMs: number
): AfterlifeState {
  let next: Map<string, AfterlifeEntry> | null = null
  for (const [id, entry] of state.afterlife) {
    if (nowMs - entry.removedAtMs < ttlMs) continue
    if (!next) next = new Map(state.afterlife)
    next.delete(id)
  }
  return next ? { afterlife: next } : state
}

export function composeRows(
  openOrders: readonly OrderInfo[],
  afterlife: ReadonlyMap<string, AfterlifeEntry>
): MyOrderRow[] {
  const rows: MyOrderRow[] = openOrders.map((order) => ({ order, status: 'open' as const }))

  const afterlifeEntries = Array.from(afterlife.values()).sort(
    (a, b) => b.removedAtMs - a.removedAtMs
  )
  for (const entry of afterlifeEntries) {
    rows.push({ order: entry.order, status: entry.status })
  }
  return rows
}

export function userPriceLevels(openOrders: readonly OrderInfo[]): Set<string> {
  const set = new Set<string>()
  for (const o of openOrders) set.add(o.price)
  return set
}
