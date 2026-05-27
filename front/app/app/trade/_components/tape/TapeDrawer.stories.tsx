import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { AuctionSummarySchema } from '@/lib/sdk'

import { TapeDrawer } from './TapeDrawer'

const sample = create(AuctionSummarySchema, {
  auctionId: '0xa1b2c3-1042',
  pair: 'ETH/USDC',
  clearingPrice: '2418.10',
  matchedVolume: '0.045300',
  matchCount: 3,
  timestampUnix: 1_700_000_000n,
})

export const Open = () => {
  const [auction, setAuction] = React.useState<typeof sample | null>(sample)
  return (
    <div className="min-h-screen bg-brand-bg p-8">
      <button
        type="button"
        onClick={() => setAuction(sample)}
        className="bg-brand-accent px-4 py-2 font-mono text-label-lg uppercase text-brand-on-accent"
      >
        [ OPEN DRAWER ]
      </button>
      <TapeDrawer auction={auction} onClose={() => setAuction(null)} />
    </div>
  )
}
