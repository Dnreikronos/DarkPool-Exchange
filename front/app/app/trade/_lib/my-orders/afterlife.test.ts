// The "afterlife" reducer keeps cancelled/filled rows on screen for a
// short TTL after the underlying store has dropped them, so the trader
// can see *what just happened*. These tests pin the diff semantics:
// removals are inferred from openOrders snapshots; cancels are signalled
// explicitly so we don't mis-classify a user-initiated cancel as a fill.

import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  appendFilled,
  composeRows,
  emptyAfterlifeState,
  markCancelled,
  pruneAfterlife,
  userPriceLevels,
} from './afterlife'

function mkOrder(overrides: Partial<OrderInfo> = {}): OrderInfo {
  return create(OrderInfoSchema, {
    id: 'o-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '1',
    remainingSize: '1',
    commitmentKey: 'mock-k',
    submittedAtUnix: 1700000000n,
    expiresAtUnix: 0n,
    ...overrides,
  })
}

describe('appendFilled', () => {
  it('records orders that left openOrders without a cancel marker as filled', () => {
    const a = mkOrder({ id: 'a' })
    const b = mkOrder({ id: 'b' })
    const prev = new Map([
      ['a', a],
      ['b', b],
    ])
    const next = new Map([['b', b]])

    const out = appendFilled(emptyAfterlifeState(), {
      prevOpenOrders: prev,
      nextOpenOrders: next,
      nowMs: 1000,
    })

    expect(out.afterlife.size).toBe(1)
    const entry = out.afterlife.get('a')
    expect(entry).toBeDefined()
    expect(entry!.status).toBe('filled')
    expect(entry!.removedAtMs).toBe(1000)
    expect(entry!.order.id).toBe('a')
  })

  it('skips orders that already have a cancelled marker', () => {
    const a = mkOrder({ id: 'a' })
    const initial = markCancelled(emptyAfterlifeState(), a, 100)

    const out = appendFilled(initial, {
      prevOpenOrders: new Map([['a', a]]),
      nextOpenOrders: new Map(),
      nowMs: 200,
    })

    const entry = out.afterlife.get('a')
    expect(entry).toBeDefined()
    expect(entry!.status).toBe('cancelled')
    // removedAtMs unchanged — the cancel marker wins.
    expect(entry!.removedAtMs).toBe(100)
  })

  it('returns the input state unchanged when nothing was removed', () => {
    const a = mkOrder({ id: 'a' })
    const same = new Map([['a', a]])
    const state = emptyAfterlifeState()

    const out = appendFilled(state, {
      prevOpenOrders: same,
      nextOpenOrders: same,
      nowMs: 1,
    })

    expect(out).toBe(state)
  })
})

describe('markCancelled', () => {
  it('captures the order snapshot at cancel time', () => {
    const o = mkOrder({ id: 'a', price: '3010' })

    const out = markCancelled(emptyAfterlifeState(), o, 500)

    const entry = out.afterlife.get('a')
    expect(entry).toBeDefined()
    expect(entry!.status).toBe('cancelled')
    expect(entry!.removedAtMs).toBe(500)
    expect(entry!.order.price).toBe('3010')
  })

  it('replaces an earlier filled marker for the same id', () => {
    // Edge case: a previous afterlife entry exists (e.g. user re-uses a
    // synthetic id). Cancel should win because it carries the user's
    // explicit intent.
    const o = mkOrder({ id: 'a' })
    const filled = appendFilled(emptyAfterlifeState(), {
      prevOpenOrders: new Map([['a', o]]),
      nextOpenOrders: new Map(),
      nowMs: 100,
    })

    const out = markCancelled(filled, o, 200)

    expect(out.afterlife.get('a')!.status).toBe('cancelled')
    expect(out.afterlife.get('a')!.removedAtMs).toBe(200)
  })
})

describe('pruneAfterlife', () => {
  it('drops entries older than ttlMs', () => {
    const a = mkOrder({ id: 'a' })
    const b = mkOrder({ id: 'b' })
    const state = markCancelled(markCancelled(emptyAfterlifeState(), a, 1000), b, 5000)

    const out = pruneAfterlife(state, /* nowMs */ 6500, /* ttlMs */ 5000)

    expect(out.afterlife.has('a')).toBe(false) // 6500 - 1000 = 5500 ≥ 5000
    expect(out.afterlife.has('b')).toBe(true) // 6500 - 5000 = 1500 < 5000
  })

  it('returns the input state unchanged when nothing expired', () => {
    const a = mkOrder({ id: 'a' })
    const state = markCancelled(emptyAfterlifeState(), a, 1000)

    const out = pruneAfterlife(state, 1500, 5000)

    expect(out).toBe(state)
  })
})

describe('composeRows', () => {
  it('places open rows first, then afterlife rows, newest first within each group', () => {
    const open1 = mkOrder({ id: 'open-1', submittedAtUnix: 100n })
    const open2 = mkOrder({ id: 'open-2', submittedAtUnix: 200n })
    const filled = mkOrder({ id: 'filled-1', submittedAtUnix: 50n })

    const afterlife = markCancelled(emptyAfterlifeState(), filled, 999).afterlife

    // openOrders are passed in store order (newest-first per mock-store contract).
    const rows = composeRows([open2, open1], afterlife)

    expect(rows.map((r) => r.order.id)).toEqual(['open-2', 'open-1', 'filled-1'])
    expect(rows.map((r) => r.status)).toEqual(['open', 'open', 'cancelled'])
  })

  it('sorts multiple afterlife entries by removedAtMs descending', () => {
    const a = mkOrder({ id: 'a' })
    const b = mkOrder({ id: 'b' })
    let s = emptyAfterlifeState()
    s = markCancelled(s, a, 1000)
    s = markCancelled(s, b, 2000)

    const rows = composeRows([], s.afterlife)

    expect(rows.map((r) => r.order.id)).toEqual(['b', 'a'])
  })
})

describe('userPriceLevels', () => {
  it('returns the set of distinct prices across openOrders', () => {
    const orders = [
      mkOrder({ id: 'a', price: '3000' }),
      mkOrder({ id: 'b', price: '3001' }),
      mkOrder({ id: 'c', price: '3000' }),
    ]
    const set = userPriceLevels(orders)
    expect(set.has('3000')).toBe(true)
    expect(set.has('3001')).toBe(true)
    expect(set.size).toBe(2)
  })

  it('is empty when there are no orders', () => {
    expect(userPriceLevels([]).size).toBe(0)
  })
})
