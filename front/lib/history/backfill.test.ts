import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory, IDBKeyRange } from 'fake-indexeddb'
import { create } from '@bufbuild/protobuf'

import {
  GetOrderResponseSchema,
  OrderInfoSchema,
  Side,
  type OrderInfo,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/sdk/client'

import { backfillTrader, getOrderOrNull } from './backfill'
import { createHistoryDb, type HistoryDb } from './db'
import { applyFill, listFills, listOrders, recordSubmittedOrder } from './repo'
import type { FillRecord, OrderRecord } from './records'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'
const NOW = 1_700_000_300

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

function remote(id: string, remainingSize: string): OrderInfo {
  return create(OrderInfoSchema, {
    id,
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '2',
    remainingSize,
    commitmentKey: 'ck-1',
    submittedAtUnix: 1_700_000_000n,
    expiresAtUnix: 1_700_000_600n,
  })
}

let db: HistoryDb

beforeEach(() => {
  db = createHistoryDb({ indexedDB: new IDBFactory(), IDBKeyRange })
})

describe('getOrderOrNull', () => {
  it('unwraps the order from a successful response', async () => {
    const client = {
      getOrder: async () => create(GetOrderResponseSchema, { order: remote('order-1', '2') }),
    }
    const result = await getOrderOrNull(client, 'order-1')
    expect(result?.id).toBe('order-1')
  })

  it('maps NOT_FOUND to null', async () => {
    const client = {
      getOrder: async () => {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.NOT_FOUND, 'order order-1 not found')
      },
    }
    await expect(getOrderOrNull(client, 'order-1')).resolves.toBeNull()
  })

  it('rethrows other errors', async () => {
    const client = {
      getOrder: async () => {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAVAILABLE, 'down')
      },
    }
    await expect(getOrderOrNull(client, 'order-1')).rejects.toThrow('down')
  })
})

describe('backfillTrader', () => {
  it('updates partially-filled resting orders and synthesizes the fill delta', async () => {
    await recordSubmittedOrder(db, order())
    const summary = await backfillTrader(db, TRADER, {
      getOrder: async (id) => remote(id, '1.5'),
      nowUnixSec: () => NOW,
    })
    expect(summary).toEqual({ checked: 1, stillOpen: 1, filled: 0, expired: 0, errors: 0 })
    const [o] = await listOrders(db, TRADER)
    expect(o.remainingSize).toBe('1.5')
    const fills = await listFills(db, TRADER)
    expect(fills).toHaveLength(1)
    expect(fills[0].size).toBe('0.5')
  })

  it('marks not-found orders before expiry as filled', async () => {
    await recordSubmittedOrder(db, order())
    const summary = await backfillTrader(db, TRADER, {
      getOrder: async () => null,
      nowUnixSec: () => NOW,
    })
    expect(summary.filled).toBe(1)
    const [o] = await listOrders(db, TRADER)
    expect(o.status).toBe('filled')
    const fills = await listFills(db, TRADER)
    expect(fills).toHaveLength(1)
    expect(fills[0].size).toBe('2')
  })

  it('marks not-found orders past expiry as expired without a fill', async () => {
    await recordSubmittedOrder(db, order({ expiresAtUnix: '1700000200' }))
    const summary = await backfillTrader(db, TRADER, {
      getOrder: async () => null,
      nowUnixSec: () => NOW,
    })
    expect(summary.expired).toBe(1)
    expect(await listFills(db, TRADER)).toHaveLength(0)
  })

  it('accounts for fills already recorded before going terminal', async () => {
    await recordSubmittedOrder(db, order())
    const prior: FillRecord = {
      fillId: 'fill-prior',
      orderId: 'order-1',
      auctionId: 'auc-1',
      trader: TRADER,
      side: Side.BUY,
      price: '3000',
      size: '0.5',
      timestampUnix: '1700000100',
    }
    await applyFill(db, prior)
    await backfillTrader(db, TRADER, {
      getOrder: async () => null,
      nowUnixSec: () => NOW,
    })
    const fills = await listFills(db, TRADER)
    expect(fills).toHaveLength(2)
    const synthesized = fills.find((f) => f.fillId !== 'fill-prior')
    expect(synthesized?.size).toBe('1.5')
  })

  it('skips records whose lookup fails and keeps going', async () => {
    await recordSubmittedOrder(db, order({ id: 'order-1' }))
    await recordSubmittedOrder(db, order({ id: 'order-2' }))
    const summary = await backfillTrader(db, TRADER, {
      getOrder: async (id) => {
        if (id === 'order-1') throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAVAILABLE, 'down')
        return remote(id, '2')
      },
      nowUnixSec: () => NOW,
    })
    expect(summary.errors).toBe(1)
    expect(summary.stillOpen).toBe(1)
    const orders = await listOrders(db, TRADER)
    expect(orders.find((o) => o.id === 'order-1')?.status).toBe('open')
  })

  it('does nothing when there are no non-terminal records', async () => {
    await recordSubmittedOrder(db, order({ status: 'filled' }))
    const summary = await backfillTrader(db, TRADER, {
      getOrder: async () => {
        throw new Error('should not be called')
      },
      nowUnixSec: () => NOW,
    })
    expect(summary.checked).toBe(0)
  })

  it('is idempotent across repeated boots', async () => {
    await recordSubmittedOrder(db, order())
    const deps = { getOrder: async () => null, nowUnixSec: () => NOW }
    await backfillTrader(db, TRADER, deps)
    await backfillTrader(db, TRADER, deps)
    expect(await listFills(db, TRADER)).toHaveLength(1)
  })
})
