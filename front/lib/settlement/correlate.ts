// Pure correlation between off-chain auction data and on-chain
// BatchSettled events (#100). The chain event only carries
// (batchId, timestamp) — batchId is a settlement-batch UUID distinct from
// the tape's auctionId — so the linkage is by timestamp proximity: a
// settlement lands within seconds of the auction it settles. The whole
// map is recomputed from the full lists on every change, which makes
// arrival order irrelevant (event-before-fill and fill-before-event both
// converge to the same result).
//
// Caveat: each panel correlates against its own anchor set (the tape
// uses the visible auctions, order panels the user's fills), so with a
// 5s auction cadence the greedy assignment can resolve the same event
// to different auctions across panels, and a neighbour auction's
// settlement can claim a fill whose own event was missed (the watcher
// is session-scoped). Accepted for the 30s-window heuristic the issue
// specifies; a shared correlation source is a possible follow-up.

/** An on-chain BatchSettled occurrence, as captured by the watcher. */
export interface SettlementEvent {
  /** bytes32 batch id from the event, 0x-prefixed. */
  batchId: string
  /** Hash of the transaction that emitted the event. */
  txHash: string
  /** The event's `timestamp` arg — unix seconds. */
  timestampUnix: bigint
}

/**
 * Anything that names an auction and when it happened. Both
 * `AuctionSummary` (tape) and `Fill` (order history) satisfy this shape
 * structurally.
 */
export interface SettlementAnchor {
  auctionId: string
  timestampUnix: bigint
}

/** Max |auction − event| timestamp gap considered the same settlement. */
export const SETTLEMENT_WINDOW_SECONDS = 30n

function absDiff(a: bigint, b: bigint): bigint {
  return a > b ? a - b : b - a
}

interface Candidate {
  anchor: SettlementAnchor
  event: SettlementEvent
  dist: bigint
}

/**
 * Greedy one-to-one nearest-first matching between auctions and
 * settlement events within `windowSeconds`. Deterministic regardless of
 * input array order: ties break toward the earlier auction, then ids.
 */
export function correlateSettlements(
  anchors: readonly SettlementAnchor[],
  events: readonly SettlementEvent[],
  windowSeconds: bigint = SETTLEMENT_WINDOW_SECONDS
): ReadonlyMap<string, SettlementEvent> {
  const uniqueAnchors = dedupeAnchors(anchors)
  const uniqueEvents = dedupeEvents(events)

  const candidates: Candidate[] = []
  for (const anchor of uniqueAnchors) {
    for (const event of uniqueEvents) {
      const dist = absDiff(anchor.timestampUnix, event.timestampUnix)
      if (dist <= windowSeconds) candidates.push({ anchor, event, dist })
    }
  }

  candidates.sort(
    (a, b) =>
      cmpBigint(a.dist, b.dist) ||
      cmpBigint(a.anchor.timestampUnix, b.anchor.timestampUnix) ||
      cmpString(a.anchor.auctionId, b.anchor.auctionId) ||
      cmpBigint(a.event.timestampUnix, b.event.timestampUnix) ||
      cmpString(a.event.batchId, b.event.batchId)
  )

  const links = new Map<string, SettlementEvent>()
  const claimedEvents = new Set<string>()
  for (const { anchor, event } of candidates) {
    if (links.has(anchor.auctionId) || claimedEvents.has(event.batchId)) continue
    links.set(anchor.auctionId, event)
    claimedEvents.add(event.batchId)
  }
  return links
}

/** Keep one anchor per auctionId — the earliest-timestamped occurrence. */
function dedupeAnchors(anchors: readonly SettlementAnchor[]): SettlementAnchor[] {
  const byId = new Map<string, SettlementAnchor>()
  for (const anchor of anchors) {
    const prev = byId.get(anchor.auctionId)
    if (!prev || anchor.timestampUnix < prev.timestampUnix) byId.set(anchor.auctionId, anchor)
  }
  return [...byId.values()]
}

/** Keep one event per batchId — the earliest-timestamped occurrence. */
function dedupeEvents(events: readonly SettlementEvent[]): SettlementEvent[] {
  const byId = new Map<string, SettlementEvent>()
  for (const event of events) {
    const prev = byId.get(event.batchId)
    if (!prev || event.timestampUnix < prev.timestampUnix) byId.set(event.batchId, event)
  }
  return [...byId.values()]
}

function cmpBigint(a: bigint, b: bigint): number {
  return a < b ? -1 : a > b ? 1 : 0
}

function cmpString(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0
}
