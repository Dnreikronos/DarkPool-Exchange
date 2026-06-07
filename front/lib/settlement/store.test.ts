import { describe, expect, it } from 'vitest'

import { createSettlementStore, SETTLEMENT_EVENTS_CAP } from './store'
import type { SettlementEvent } from './correlate'

function event(batchId: string, timestampUnix: bigint): SettlementEvent {
  return { batchId, txHash: `0xtx-${batchId}`, timestampUnix }
}

describe('settlement store', () => {
  it('starts empty', () => {
    const store = createSettlementStore()
    expect(store.getState().events).toEqual([])
  })

  it('appends events newest first', () => {
    const store = createSettlementStore()
    store.getState().addEvents([event('0xa', 100n)])
    store.getState().addEvents([event('0xb', 200n)])
    expect(store.getState().events.map((e) => e.batchId)).toEqual(['0xb', '0xa'])
  })

  it('dedupes by batchId across calls (watcher replays)', () => {
    const store = createSettlementStore()
    store.getState().addEvents([event('0xa', 100n)])
    store.getState().addEvents([event('0xa', 100n), event('0xb', 200n)])
    expect(store.getState().events.map((e) => e.batchId)).toEqual(['0xb', '0xa'])
  })

  it('does not notify subscribers when every event is a duplicate', () => {
    const store = createSettlementStore()
    store.getState().addEvents([event('0xa', 100n)])
    const before = store.getState().events
    store.getState().addEvents([event('0xa', 100n)])
    expect(store.getState().events).toBe(before)
  })

  it('caps retained events, dropping the oldest', () => {
    const store = createSettlementStore()
    const batch: SettlementEvent[] = []
    for (let i = 0; i < SETTLEMENT_EVENTS_CAP + 5; i++) {
      batch.push(event(`0x${i}`, BigInt(i)))
    }
    store.getState().addEvents(batch)
    const events = store.getState().events
    expect(events).toHaveLength(SETTLEMENT_EVENTS_CAP)
    // Oldest (lowest timestamps) dropped.
    expect(events.some((e) => e.batchId === '0x0')).toBe(false)
  })
})
