import { create } from '@bufbuild/protobuf'
import { describe, expect, it } from 'vitest'

import {
  GetAuctionHistoryResponseSchema,
  type GetAuctionHistoryResponse,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { auctionsFromQuery } from './auctions'

function makeResponse(count: number): GetAuctionHistoryResponse {
  return create(GetAuctionHistoryResponseSchema, {
    auctions: Array.from({ length: count }, (_, i) => ({
      auctionId: `a-${i}`,
      pair: 'ETH/USDC',
      clearingPrice: '3000',
      matchedVolume: '1',
      matchCount: 1,
      timestampUnix: BigInt(1_700_000_000 + i),
    })),
  })
}

describe('auctionsFromQuery', () => {
  it('returns the response auctions array when data is present', () => {
    const data = makeResponse(3)
    expect(auctionsFromQuery({ data })).toBe(data.auctions)
  })

  it('returns an empty list while the first request is in flight (no data yet)', () => {
    expect(auctionsFromQuery({ data: undefined })).toEqual([])
  })

  it('returns an empty list when the backend reports zero auctions (fresh deployment)', () => {
    const data = makeResponse(0)
    const auctions = auctionsFromQuery({ data })
    expect(auctions).toHaveLength(0)
  })
})
