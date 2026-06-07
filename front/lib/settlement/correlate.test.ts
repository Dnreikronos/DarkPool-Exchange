import { describe, expect, it } from 'vitest'

import {
  correlateSettlements,
  SETTLEMENT_WINDOW_SECONDS,
  type SettlementAnchor,
  type SettlementEvent,
} from './correlate'

function anchor(auctionId: string, timestampUnix: bigint): SettlementAnchor {
  return { auctionId, timestampUnix }
}

function event(batchId: string, timestampUnix: bigint, txHash = `0xtx-${batchId}`): SettlementEvent {
  return { batchId, txHash, timestampUnix }
}

describe('correlateSettlements', () => {
  it('links an auction to a settlement event within the window', () => {
    const links = correlateSettlements(
      [anchor('auction-1', 1000n)],
      [event('0xbatch1', 1005n)]
    )
    expect(links.get('auction-1')?.batchId).toBe('0xbatch1')
  })

  it('does not link an event outside the 30s window', () => {
    const links = correlateSettlements(
      [anchor('auction-1', 1000n)],
      [event('0xbatch1', 1031n)]
    )
    expect(links.size).toBe(0)
  })

  it('links at exactly the window boundary (inclusive)', () => {
    const links = correlateSettlements(
      [anchor('auction-1', 1000n)],
      [event('0xbatch1', 1000n + SETTLEMENT_WINDOW_SECONDS)]
    )
    expect(links.get('auction-1')?.batchId).toBe('0xbatch1')
  })

  it('links when the event timestamp precedes the auction timestamp', () => {
    // Out-of-order arrival: the chain event can land before the engine
    // reports the auction. Correlation is symmetric around the anchor.
    const links = correlateSettlements(
      [anchor('auction-1', 1000n)],
      [event('0xbatch1', 985n)]
    )
    expect(links.get('auction-1')?.batchId).toBe('0xbatch1')
  })

  it('assigns an event to the nearest auction when several are in window', () => {
    const links = correlateSettlements(
      [anchor('auction-near', 1004n), anchor('auction-far', 1020n)],
      [event('0xbatch1', 1005n)]
    )
    expect(links.get('auction-near')?.batchId).toBe('0xbatch1')
    expect(links.has('auction-far')).toBe(false)
  })

  it('matches one-to-one: each event links at most one auction and vice versa', () => {
    // Two auctions, two events. Greedy nearest-first pairs (a1,e1) then
    // the remaining (a2,e2) even though e1 is also within a2's window.
    const links = correlateSettlements(
      [anchor('auction-1', 1000n), anchor('auction-2', 1010n)],
      [event('0xbatch1', 1001n), event('0xbatch2', 1012n)]
    )
    expect(links.get('auction-1')?.batchId).toBe('0xbatch1')
    expect(links.get('auction-2')?.batchId).toBe('0xbatch2')
  })

  it('is independent of input array order', () => {
    const anchors = [anchor('auction-1', 1000n), anchor('auction-2', 1010n)]
    const events = [event('0xbatch1', 1001n), event('0xbatch2', 1012n)]
    const forward = correlateSettlements(anchors, events)
    const reversed = correlateSettlements([...anchors].reverse(), [...events].reverse())
    expect(reversed).toEqual(forward)
  })

  it('dedupes anchors sharing an auctionId (e.g. multiple fills of one auction)', () => {
    const links = correlateSettlements(
      [anchor('auction-1', 1000n), anchor('auction-1', 1002n)],
      [event('0xbatch1', 1001n)]
    )
    expect(links.size).toBe(1)
    expect(links.get('auction-1')?.batchId).toBe('0xbatch1')
  })

  it('dedupes events sharing a batchId (watcher replays)', () => {
    const links = correlateSettlements(
      [anchor('auction-1', 1000n), anchor('auction-2', 1010n)],
      [event('0xbatch1', 1001n), event('0xbatch1', 1001n)]
    )
    expect(links.size).toBe(1)
  })

  it('breaks distance ties deterministically toward the earlier auction', () => {
    const links = correlateSettlements(
      [anchor('auction-late', 1010n), anchor('auction-early', 1000n)],
      [event('0xbatch1', 1005n)]
    )
    expect(links.get('auction-early')?.batchId).toBe('0xbatch1')
    expect(links.has('auction-late')).toBe(false)
  })

  it('returns an empty map for empty inputs', () => {
    expect(correlateSettlements([], []).size).toBe(0)
    expect(correlateSettlements([anchor('a', 1n)], []).size).toBe(0)
    expect(correlateSettlements([], [event('0xb', 1n)]).size).toBe(0)
  })

  it('honors a custom window', () => {
    const links = correlateSettlements([anchor('auction-1', 1000n)], [event('0xbatch1', 1004n)], 3n)
    expect(links.size).toBe(0)
  })
})
