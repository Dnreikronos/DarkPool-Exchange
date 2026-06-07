import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import { AuctionEventSchema, AuctionSummarySchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  addLive,
  auctionEventToSummary,
  emptyFeed,
  mergeHistory,
  selectAuctions,
  MAX_RETAINED_AUCTIONS,
} from './feed'

function summary(id: string, ts: bigint) {
  return create(AuctionSummarySchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '3000.5',
    matchedVolume: '2.5',
    matchCount: 3,
    timestampUnix: ts,
  })
}

function event(id: string, ts: bigint) {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '3000.5',
    matchedVolume: '2.5',
    matchCount: 3,
    timestampUnix: ts,
  })
}

describe('feed reducer', () => {
  it('auctionEventToSummary preserves string decimals and the bigint timestamp', () => {
    const s = auctionEventToSummary(event('a1', 5n))
    expect(s.auctionId).toBe('a1')
    expect(s.clearingPrice).toBe('3000.5')
    expect(s.matchedVolume).toBe('2.5')
    expect(s.matchCount).toBe(3)
    expect(s.timestampUnix).toBe(5n)
  })

  it('mergeHistory dedups by auctionId (latest wins)', () => {
    let state = emptyFeed()
    state = mergeHistory(state, [summary('a1', 1n), summary('a2', 2n)])
    state = mergeHistory(state, [summary('a1', 9n)])
    const out = selectAuctions(state, 50)
    expect(out.map((a) => a.auctionId)).toEqual(['a1', 'a2'])
    expect(out[0].timestampUnix).toBe(9n)
  })

  it('mergeHistory keeps the newer row when a poll returns an older snapshot', () => {
    let state = emptyFeed()
    // A live SSE update lands first…
    state = addLive(state, event('a1', 9n))
    // …then a slower REST backfill returns a stale snapshot of the same auction.
    state = mergeHistory(state, [summary('a1', 4n)])
    expect(state.byId.get('a1')?.timestampUnix).toBe(9n)
  })

  it('addLive upserts and selectAuctions sorts newest-first', () => {
    let state = emptyFeed()
    state = mergeHistory(state, [summary('a1', 1n)])
    state = addLive(state, event('a2', 5n))
    state = addLive(state, event('a1', 1n)) // dup id, no growth
    const out = selectAuctions(state, 50)
    expect(out.map((a) => a.auctionId)).toEqual(['a2', 'a1'])
  })

  it('selectAuctions slices to the limit', () => {
    let state = emptyFeed()
    for (let i = 0; i < 10; i++) state = addLive(state, event(`a${i}`, BigInt(i)))
    expect(selectAuctions(state, 3)).toHaveLength(3)
  })

  it('prunes to MAX_RETAINED_AUCTIONS keeping the newest', () => {
    let state = emptyFeed()
    for (let i = 0; i < MAX_RETAINED_AUCTIONS + 50; i++) {
      state = addLive(state, event(`a${i}`, BigInt(i)))
    }
    const out = selectAuctions(state, MAX_RETAINED_AUCTIONS + 100)
    expect(out).toHaveLength(MAX_RETAINED_AUCTIONS)
    expect(out[0].auctionId).toBe(`a${MAX_RETAINED_AUCTIONS + 49}`) // newest
  })
})
