import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory, IDBKeyRange } from 'fake-indexeddb'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { createHistoryDb, type HistoryDb } from './db'
import {
  applyFill,
  applyReconciliation,
  fillsForOrder,
  listFills,
  listNonTerminalOrders,
  listOrders,
  markOrderCancelled,
  recordSubmittedOrder,
} from './repo'
import type { FillRecord, OrderRecord } from './records'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'
const OTHER = '1111111111111111111111111111111111111111'

function order(overrides: Partial<OrderRecord> = {}): OrderRecord {
  return {
    id: 'order-1',
    trader: TRADER,
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '2',
    remainingSize: '2',
    status: 'open',
    commitmentKey: 'ck-1',
    submittedAtUnix: '1700000000',
    expiresAtUnix: '1700000600',
    ...overrides,
  }
}

function fill(overrides: Partial<FillRecord> = {}): FillRecord {
  return {
    fillId: 'fill-1',
    orderId: 'order-1',
    auctionId: 'auc-1',
    trader: TRADER,
    side: Side.BUY,
    price: '2999',
    size: '0.5',
    timestampUnix: '1700000100',
    ...overrides,
  }
}

let db: HistoryDb

beforeEach(() => {
  // A fresh IDBFactory per test = a pristine database, no cross-test state.
  db = createHistoryDb({ indexedDB: new IDBFactory(), IDBKeyRange })
})

afterEach(() => {
  db.close()
})

describe('recordSubmittedOrder / listOrders', () => {
  it('persists and lists per-trader orders', async () => {
    await recordSubmittedOrder(db, order())
    await recordSubmittedOrder(db, order({ id: 'order-2', trader: OTHER }))
    const mine = await listOrders(db, TRADER)
    expect(mine.map((o) => o.id)).toEqual(['order-1'])
  })

  it('is an upsert — re-recording the same id does not duplicate', async () => {
    await recordSubmittedOrder(db, order())
    await recordSubmittedOrder(db, order({ remainingSize: '1' }))
    const mine = await listOrders(db, TRADER)
    expect(mine).toHaveLength(1)
    expect(mine[0].remainingSize).toBe('1')
  })
})

describe('listNonTerminalOrders', () => {
  it('returns only open orders for the trader', async () => {
    await recordSubmittedOrder(db, order())
    await recordSubmittedOrder(db, order({ id: 'order-2', status: 'filled' }))
    await recordSubmittedOrder(db, order({ id: 'order-3', status: 'cancelled' }))
    await recordSubmittedOrder(db, order({ id: 'order-4', trader: OTHER }))
    const open = await listNonTerminalOrders(db, TRADER)
    expect(open.map((o) => o.id)).toEqual(['order-1'])
  })
})

describe('markOrderCancelled', () => {
  it('flips an open order to cancelled', async () => {
    await recordSubmittedOrder(db, order())
    await markOrderCancelled(db, 'order-1')
    const [o] = await listOrders(db, TRADER)
    expect(o.status).toBe('cancelled')
  })

  it('does not resurrect an already-terminal order', async () => {
    await recordSubmittedOrder(db, order({ status: 'filled' }))
    await markOrderCancelled(db, 'order-1')
    const [o] = await listOrders(db, TRADER)
    expect(o.status).toBe('filled')
  })

  it('is a no-op for unknown ids', async () => {
    await expect(markOrderCancelled(db, 'nope')).resolves.toBeUndefined()
  })
})

describe('applyFill', () => {
  it('stores the fill and decrements the order remainingSize', async () => {
    await recordSubmittedOrder(db, order())
    await applyFill(db, fill())
    const [o] = await listOrders(db, TRADER)
    expect(o.remainingSize).toBe('1.5')
    expect(o.status).toBe('open')
    expect(await fillsForOrder(db, 'order-1')).toHaveLength(1)
  })

  it('marks the order filled when the remainder reaches zero', async () => {
    await recordSubmittedOrder(db, order({ size: '0.5', remainingSize: '0.5' }))
    await applyFill(db, fill())
    const [o] = await listOrders(db, TRADER)
    expect(o.remainingSize).toBe('0')
    expect(o.status).toBe('filled')
  })

  it('is idempotent per fillId', async () => {
    await recordSubmittedOrder(db, order())
    await applyFill(db, fill())
    await applyFill(db, fill())
    const [o] = await listOrders(db, TRADER)
    expect(o.remainingSize).toBe('1.5')
    expect(await fillsForOrder(db, 'order-1')).toHaveLength(1)
  })

  it('stores the fill even when the order record is missing', async () => {
    await applyFill(db, fill())
    expect(await listFills(db, TRADER)).toHaveLength(1)
  })
})

describe('listFills', () => {
  it('returns the trader fills newest-first with numeric timestamp ordering', async () => {
    // '99' sorts after '100' lexicographically — the repo must compare numerically.
    await applyFill(db, fill({ fillId: 'f-a', timestampUnix: '99' }))
    await applyFill(db, fill({ fillId: 'f-b', timestampUnix: '100' }))
    await applyFill(db, fill({ fillId: 'f-c', timestampUnix: '98', trader: OTHER }))
    const fills = await listFills(db, TRADER)
    expect(fills.map((f) => f.fillId)).toEqual(['f-b', 'f-a'])
  })
})

describe('applyReconciliation', () => {
  it('persists the updated record and the synthesized fill together', async () => {
    await recordSubmittedOrder(db, order())
    await applyReconciliation(db, {
      record: order({ status: 'filled', remainingSize: '0' }),
      fill: fill({ fillId: 'backfill-order-1-0', size: '2' }),
    })
    const [o] = await listOrders(db, TRADER)
    expect(o.status).toBe('filled')
    expect(await fillsForOrder(db, 'order-1')).toHaveLength(1)
  })

  it('accepts a null fill', async () => {
    await recordSubmittedOrder(db, order())
    await applyReconciliation(db, { record: order({ status: 'expired' }), fill: null })
    const [o] = await listOrders(db, TRADER)
    expect(o.status).toBe('expired')
    expect(await listFills(db, TRADER)).toHaveLength(0)
  })

  it('re-applying the same reconciliation does not duplicate the fill', async () => {
    await recordSubmittedOrder(db, order())
    const result = {
      record: order({ status: 'filled' as const, remainingSize: '0' }),
      fill: fill({ fillId: 'backfill-order-1-0', size: '2' }),
    }
    await applyReconciliation(db, result)
    await applyReconciliation(db, result)
    expect(await fillsForOrder(db, 'order-1')).toHaveLength(1)
  })
})
