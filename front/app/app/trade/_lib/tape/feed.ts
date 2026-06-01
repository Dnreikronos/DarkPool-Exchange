// Pure merge layer for the tape: one keyed store fed by BOTH the REST history
// poll and the live SSE stream. Dedup is by auctionId, so an event already in
// history (or vice-versa) is a no-op. State is immutable (new Map per action)
// so React detects the change.

import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type {
  AuctionEvent,
  AuctionSummary,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

// Bound memory for a long-lived live stream. A 5 s cadence fills 200 rows in
// ~17 min; older clears scroll out of any realistic viewport.
export const MAX_RETAINED_AUCTIONS = 200

export interface FeedState {
  readonly byId: ReadonlyMap<string, AuctionSummary>
}

export function emptyFeed(): FeedState {
  return { byId: new Map() }
}

export function auctionEventToSummary(event: AuctionEvent): AuctionSummary {
  return create(AuctionSummarySchema, {
    auctionId: event.auctionId,
    pair: event.pair,
    clearingPrice: event.clearingPrice,
    matchedVolume: event.matchedVolume,
    matchCount: event.matchCount,
    timestampUnix: event.timestampUnix,
  })
}

// Newest first; stable tiebreak on id so equal-timestamp rows don't jitter.
function cmpDesc(a: AuctionSummary, b: AuctionSummary): number {
  if (a.timestampUnix === b.timestampUnix) {
    return a.auctionId < b.auctionId ? 1 : a.auctionId > b.auctionId ? -1 : 0
  }
  return a.timestampUnix > b.timestampUnix ? -1 : 1
}

function prune(map: Map<string, AuctionSummary>): Map<string, AuctionSummary> {
  if (map.size <= MAX_RETAINED_AUCTIONS) return map
  const newest = [...map.values()].sort(cmpDesc).slice(0, MAX_RETAINED_AUCTIONS)
  return new Map(newest.map((a) => [a.auctionId, a]))
}

export function mergeHistory(
  state: FeedState,
  summaries: readonly AuctionSummary[]
): FeedState {
  if (summaries.length === 0) return state
  const next = new Map(state.byId)
  for (const s of summaries) next.set(s.auctionId, s)
  return { byId: prune(next) }
}

export function addLive(state: FeedState, event: AuctionEvent): FeedState {
  const summary = auctionEventToSummary(event)
  const next = new Map(state.byId)
  next.set(summary.auctionId, summary)
  return { byId: prune(next) }
}

export function selectAuctions(state: FeedState, limit: number): AuctionSummary[] {
  const sorted = [...state.byId.values()].sort(cmpDesc)
  return limit > 0 ? sorted.slice(0, limit) : sorted
}
