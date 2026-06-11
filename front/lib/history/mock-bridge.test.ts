import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory, IDBKeyRange } from 'fake-indexeddb'

import { createMockStore } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { createHistoryDb, type HistoryDb } from './db'
import { startMockHistoryMirror, type MockHistoryMirror } from './mock-bridge'
import { listFills, listOrders } from './repo'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'
const FROZEN_NOW = 1_700_000_000

let db: HistoryDb
let mirror: MockHistoryMirror | null = null

beforeEach(() => {
  db = createHistoryDb({ indexedDB: new IDBFactory(), IDBKeyRange })
})

afterEach(() => {
  mirror?.stop()
  mirror = null
  db.close()
})

function newStore() {
  return createMockStore({ seed: 1, now: () => FROZEN_NOW, auctionHistory: 0 })
}

describe('startMockHistoryMirror', () => {
  it('records a placed order keyed by the current trader', async () => {
    const store = newStore()
    mirror = startMockHistoryMirror(store, { db, getTrader: () => TRADER })
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    await mirror.flush()
    const orders = await listOrders(db, TRADER)
    expect(orders.map((o) => o.id)).toEqual([placed.id])
    expect(orders[0].status).toBe('open')
    expect(orders[0].price).toBe('3000')
  })

  it('mirrors an auction fill and marks the order filled', async () => {
    const store = newStore()
    mirror = startMockHistoryMirror(store, { db, getTrader: () => TRADER })
    // A buy priced far above any sampled clearing price always matches.
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '1000000', size: '1' })
    store.getState().runAuction()
    await mirror.flush()
    expect(store.getState().fillHistory).toHaveLength(1)

    const fills = await listFills(db, TRADER)
    expect(fills).toHaveLength(1)
    expect(fills[0].orderId).toBe(placed.id)
    const [order] = await listOrders(db, TRADER)
    expect(order.status).toBe('filled')
    expect(order.remainingSize).toBe('0')
  })

  it('marks an order cancelled when it leaves the book without a fill', async () => {
    const store = newStore()
    mirror = startMockHistoryMirror(store, { db, getTrader: () => TRADER })
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    store.getState().cancelOrder(placed.id)
    await mirror.flush()
    const [order] = await listOrders(db, TRADER)
    expect(order.status).toBe('cancelled')
    expect(await listFills(db, TRADER)).toHaveLength(0)
  })

  it('records nothing while disconnected', async () => {
    const store = newStore()
    mirror = startMockHistoryMirror(store, { db, getTrader: () => null })
    store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    await mirror.flush()
    expect(await listOrders(db, TRADER)).toHaveLength(0)
  })

  it('captures orders already resting when the mirror starts', async () => {
    const store = newStore()
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    mirror = startMockHistoryMirror(store, { db, getTrader: () => TRADER })
    await mirror.flush()
    const orders = await listOrders(db, TRADER)
    expect(orders.map((o) => o.id)).toEqual([placed.id])
  })

  it('stops mirroring after stop()', async () => {
    const store = newStore()
    mirror = startMockHistoryMirror(store, { db, getTrader: () => TRADER })
    mirror.stop()
    store.getState().placeOrder({ side: Side.BUY, price: '3000', size: '1' })
    await mirror.flush()
    expect(await listOrders(db, TRADER)).toHaveLength(0)
  })
})
