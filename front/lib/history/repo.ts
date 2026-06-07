// Async data access over HistoryDb. Thin by design: anything with logic
// worth testing in isolation lives in records.ts / reconcile.ts; these
// functions just move records in and out of Dexie tables.

import { Decimal } from '@/lib/units'

import type { HistoryDb } from './db'
import type { ReconcileResult } from './reconcile'
import { isTerminal, type FillRecord, type OrderRecord } from './records'

/** Upsert an order record (idempotent on id). */
export async function recordSubmittedOrder(db: HistoryDb, record: OrderRecord): Promise<void> {
  await db.orders.put(record)
}

export async function listOrders(db: HistoryDb, trader: string): Promise<OrderRecord[]> {
  return db.orders.where('trader').equals(trader).toArray()
}

/** The boot backfill's worklist: every order we last saw resting. */
export async function listNonTerminalOrders(db: HistoryDb, trader: string): Promise<OrderRecord[]> {
  return db.orders.where('[trader+status]').equals([trader, 'open']).toArray()
}

/** Flip an open order to cancelled. Terminal records and unknown ids are left alone. */
export async function markOrderCancelled(db: HistoryDb, orderId: string): Promise<void> {
  await db.transaction('rw', db.orders, async () => {
    const record = await db.orders.get(orderId)
    if (!record || isTerminal(record.status)) return
    await db.orders.put({ ...record, status: 'cancelled' })
  })
}

/**
 * Record an observed fill and roll its size into the order: remainingSize
 * decreases, status flips to filled at zero. Idempotent per fillId.
 */
export async function applyFill(db: HistoryDb, fill: FillRecord): Promise<void> {
  await db.transaction('rw', db.orders, db.fills, async () => {
    const seen = await db.fills.get(fill.fillId)
    if (seen) return
    await db.fills.put(fill)
    const order = await db.orders.get(fill.orderId)
    if (!order || isTerminal(order.status)) return
    const remaining = Decimal.max(0, new Decimal(order.remainingSize).minus(fill.size))
    await db.orders.put({
      ...order,
      remainingSize: remaining.toFixed(),
      status: remaining.isZero() ? 'filled' : 'open',
    })
  })
}

/** Trader-scoped fills, newest first (numeric timestamp ordering). */
export async function listFills(db: HistoryDb, trader: string): Promise<FillRecord[]> {
  const fills = await db.fills.where('trader').equals(trader).toArray()
  // timestampUnix is a decimal string — BigInt compare, not lexicographic.
  return fills.sort((a, b) => {
    const ta = BigInt(a.timestampUnix)
    const tb = BigInt(b.timestampUnix)
    if (ta === tb) return a.fillId < b.fillId ? -1 : a.fillId > b.fillId ? 1 : 0
    return ta < tb ? 1 : -1
  })
}

export async function fillsForOrder(db: HistoryDb, orderId: string): Promise<FillRecord[]> {
  return db.fills.where('orderId').equals(orderId).toArray()
}

/** Persist a reconcile step atomically: updated record + synthesized fill. */
export async function applyReconciliation(db: HistoryDb, result: ReconcileResult): Promise<void> {
  await db.transaction('rw', db.orders, db.fills, async () => {
    await db.orders.put(result.record)
    if (result.fill) await db.fills.put(result.fill)
  })
}
