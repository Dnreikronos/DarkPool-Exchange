import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory, IDBKeyRange } from 'fake-indexeddb'
import { create } from '@bufbuild/protobuf'

import {
  OrderInfoSchema,
  PlaceOrderResponseSchema,
  Side,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { createHistoryDb, type HistoryDb } from './db'
import { persistPlacedOrder } from './persist'
import { listOrders } from './repo'

const ADDRESS = '0xAaBbCcDdEeFf00112233445566778899AaBbCcDd'
const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'

function response() {
  return create(PlaceOrderResponseSchema, {
    order: create(OrderInfoSchema, {
      id: 'order-1',
      pair: 'ETH/USDC',
      side: Side.BUY,
      price: '3000',
      size: '1',
      remainingSize: '1',
      commitmentKey: 'ck-1',
      submittedAtUnix: 1_700_000_000n,
      expiresAtUnix: 1_700_000_600n,
    }),
  })
}

let db: HistoryDb

beforeEach(() => {
  db = createHistoryDb({ indexedDB: new IDBFactory(), IDBKeyRange })
})

describe('persistPlacedOrder', () => {
  it('records the accepted order under the normalized trader key', async () => {
    await persistPlacedOrder(response(), ADDRESS, db)
    const orders = await listOrders(db, TRADER)
    expect(orders.map((o) => o.id)).toEqual(['order-1'])
    expect(orders[0].status).toBe('open')
  })

  it('is a no-op when the response has no order', async () => {
    await persistPlacedOrder(create(PlaceOrderResponseSchema, {}), ADDRESS, db)
    expect(await listOrders(db, TRADER)).toHaveLength(0)
  })

  it('is a no-op without a trader address', async () => {
    await persistPlacedOrder(response(), null, db)
    expect(await listOrders(db, TRADER)).toHaveLength(0)
  })

  it('swallows persistence failures (best-effort)', async () => {
    db.close()
    await expect(persistPlacedOrder(response(), ADDRESS, db)).resolves.toBeUndefined()
  })
})
