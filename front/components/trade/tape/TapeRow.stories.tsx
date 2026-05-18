import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk'

import { TapeRow } from './TapeRow'

const NOW = 1_700_000_120

function mkAuction(opts: Partial<{
  auctionId: string
  clearingPrice: string
  matchedVolume: string
  matchCount: number
  ageSeconds: number
}> = {}) {
  return create(AuctionSummarySchema, {
    auctionId: opts.auctionId ?? 'a-001',
    pair: 'ETH/USDC',
    clearingPrice: opts.clearingPrice ?? '2418.10',
    matchedVolume: opts.matchedVolume ?? '0.0453',
    matchCount: opts.matchCount ?? 3,
    timestampUnix: BigInt(NOW - (opts.ageSeconds ?? 5)),
  })
}

const noop = (): void => undefined

export const SingleRow = () => (
  <ol className="w-[320px] border border-brand-border bg-brand-bg">
    <TapeRow auction={mkAuction()} nowUnixSeconds={NOW} onSelect={noop} />
  </ol>
)

export const ListOfRows = () => (
  <ol className="w-[320px] border border-brand-border bg-brand-bg">
    <TapeRow
      auction={mkAuction({ auctionId: 'a-100', ageSeconds: 2, clearingPrice: '2419.85', matchedVolume: '0.0121', matchCount: 1 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-099', ageSeconds: 7, clearingPrice: '12345.6789', matchedVolume: '0.089', matchCount: 5 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-098', ageSeconds: 17, matchedVolume: '0', matchCount: 0 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
    <TapeRow
      auction={mkAuction({ auctionId: 'a-097', ageSeconds: 65, clearingPrice: '2417.42', matchedVolume: '0.0023', matchCount: 1 })}
      nowUnixSeconds={NOW}
      onSelect={noop}
    />
  </ol>
)
