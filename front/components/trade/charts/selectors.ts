import type {
  AuctionSummary,
  GetOrderBookResponse,
  PriceLevel,
} from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb.js'
import { Decimal } from '../../../lib/units'

export interface DepthPoint {
  /** Plotting value (float). */
  price: number
  /** Plotting value (float). Cumulative size from best-of-book out to this level. */
  cumulative: number
  /** Canonical wire string for tooltips / labels. */
  priceStr: string
  /** Canonical wire string for tooltips / labels. */
  cumulativeStr: string
}

export interface DepthSeries {
  bids: DepthPoint[]
  asks: DepthPoint[]
  /** Midpoint between best bid and best ask, or null if either side is empty. */
  midPrice: number | null
  midPriceStr: string | null
  /** Shared y-domain max so bids and asks render on a single scale. */
  maxCumulative: number
  /** Best ask − best bid in quote units, or null if either side is empty. */
  spread: number | null
}

function cumulate(levels: readonly PriceLevel[]): DepthPoint[] {
  let running = new Decimal(0)
  const out: DepthPoint[] = []
  for (const lvl of levels) {
    running = running.plus(lvl.totalSize)
    out.push({
      price: Number(lvl.price),
      cumulative: Number(running),
      priceStr: lvl.price,
      cumulativeStr: running.toFixed(),
    })
  }
  return out
}

export function buildDepthSeries(book: GetOrderBookResponse): DepthSeries {
  const bids = cumulate(book.bids)
  const asks = cumulate(book.asks)

  const bestBid = book.bids[0]?.price ?? null
  const bestAsk = book.asks[0]?.price ?? null
  const haveBoth = bestBid !== null && bestAsk !== null

  const midPriceDec = haveBoth ? new Decimal(bestBid).plus(bestAsk).div(2) : null
  const spreadDec = haveBoth ? new Decimal(bestAsk).minus(bestBid) : null

  const maxCumulative = Math.max(
    bids.length > 0 ? bids[bids.length - 1].cumulative : 0,
    asks.length > 0 ? asks[asks.length - 1].cumulative : 0
  )

  return {
    bids,
    asks,
    midPrice: midPriceDec ? Number(midPriceDec) : null,
    midPriceStr: midPriceDec ? midPriceDec.toFixed() : null,
    maxCumulative,
    spread: spreadDec ? Number(spreadDec) : null,
  }
}

export type Timeframe = '1m' | '5m' | '1h'

export const TIMEFRAME_MS: Record<Timeframe, number> = {
  '1m': 60_000,
  '5m': 5 * 60_000,
  '1h': 60 * 60_000,
}

export const TIMEFRAMES: readonly Timeframe[] = ['1m', '5m', '1h'] as const

export interface PricePoint {
  /** Unix seconds — what lightweight-charts ingests as `time`. */
  time: number
  value: number
}

/**
 * Project `auctions` (newest first per the mock store contract) into the
 * chronological series lightweight-charts wants, capped to the window.
 *
 * `nowUnixSec` is injectable so callers using a frozen clock (tests, SSR)
 * pin the same boundary the auctions were generated against.
 *
 * Same-second collisions are coalesced — lightweight-charts rejects two
 * points at identical `time`, so we keep the newest auction per second
 * (matches what a chart reader would expect: the latest print wins).
 */
export function selectAuctionsInWindow(
  auctions: readonly AuctionSummary[],
  windowMs: number,
  nowUnixSec: number
): PricePoint[] {
  const cutoff = nowUnixSec - Math.floor(windowMs / 1000)
  const byTime = new Map<number, number>()
  // auctions are newest-first; iterating in order means the first write per
  // timestamp is the newest, and we skip stale duplicates after that.
  for (const a of auctions) {
    const t = Number(a.timestampUnix)
    if (t < cutoff) continue
    if (byTime.has(t)) continue
    byTime.set(t, Number(a.clearingPrice))
  }
  const points: PricePoint[] = []
  for (const [time, value] of byTime) {
    points.push({ time, value })
  }
  points.sort((a, b) => a.time - b.time)
  return points
}
