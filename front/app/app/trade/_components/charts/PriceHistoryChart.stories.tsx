import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'
import type { AuctionSummary } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'

import { PriceHistoryChart } from './PriceHistoryChart'

const NOW = 1_700_000_000

function auction(offsetSec: number, clearingPrice: string): AuctionSummary {
  return create(AuctionSummarySchema, {
    auctionId: `a-${offsetSec}`,
    pair: 'ETH/USDC',
    clearingPrice,
    matchedVolume: '1',
    matchCount: 1,
    timestampUnix: BigInt(NOW - offsetSec),
  })
}

// Build a deterministic walk that drifts around a target mean.
function walk(count: number, stepSec: number, start: number, jitter: number): AuctionSummary[] {
  let v = start
  const out: AuctionSummary[] = []
  for (let i = 0; i < count; i++) {
    v += Math.sin(i * 0.7) * jitter + Math.cos(i * 0.3) * (jitter / 2)
    out.push(auction((count - i) * stepSec, v.toFixed(2)))
  }
  // newest first per the mock-store contract
  return out.reverse()
}

const FRAME = 'border border-brand-border bg-brand-surface w-[640px] h-[300px]'

export const OneMinuteWindow = () => (
  <div className={FRAME}>
    <PriceHistoryChart
      auctionsOverride={walk(20, 5, 3000, 4)}
      nowUnixSec={NOW}
      defaultTimeframe="1m"
    />
  </div>
)

export const FiveMinuteWindow = () => (
  <div className={FRAME}>
    <PriceHistoryChart
      auctionsOverride={walk(80, 5, 3000, 8)}
      nowUnixSec={NOW}
      defaultTimeframe="5m"
    />
  </div>
)

export const OneHourWindow = () => (
  <div className={FRAME}>
    <PriceHistoryChart
      auctionsOverride={walk(180, 30, 3000, 12)}
      nowUnixSec={NOW}
      defaultTimeframe="1h"
    />
  </div>
)

export const SingleAuctionEmpty = () => (
  <div className={FRAME}>
    <PriceHistoryChart
      auctionsOverride={[auction(2, '3000')]}
      nowUnixSec={NOW}
      defaultTimeframe="1m"
    />
  </div>
)

export const NoAuctionsEmpty = () => (
  <div className={FRAME}>
    <PriceHistoryChart auctionsOverride={[]} nowUnixSec={NOW} defaultTimeframe="1m" />
  </div>
)
