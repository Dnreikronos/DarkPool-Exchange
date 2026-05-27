// Pure CSV serializer for the fill-history table. Keeps export logic out
// of the React tree so it can be unit-tested without a DOM and re-used
// by I2.11 (#101) when fill history moves into IndexedDB.

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'

export const CSV_HEADER = 'timestamp,side,price,size,fill_id,order_id,auction_id'

function quote(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`
  }
  return value
}

function sideLabel(side: Side): string {
  switch (side) {
    case Side.BUY:
      return 'BUY'
    case Side.SELL:
      return 'SELL'
    default:
      return 'UNSPECIFIED'
  }
}

export function fillsToCsv(fills: readonly Fill[]): string {
  const rows = fills.map((f) => {
    const ts = new Date(Number(f.timestampUnix) * 1000).toISOString()
    return [ts, sideLabel(f.side), f.price, f.size, f.fillId, f.orderId, f.auctionId]
      .map(quote)
      .join(',')
  })
  return `${CSV_HEADER}\n${rows.join('\n')}${rows.length > 0 ? '\n' : ''}`
}
