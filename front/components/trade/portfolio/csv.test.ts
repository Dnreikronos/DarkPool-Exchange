import { describe, expect, it } from 'vitest'

import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '../../../lib/mock-store'

import { CSV_HEADER, fillsToCsv } from './csv'

function fill(partial: Partial<Fill>): Fill {
  return {
    fillId: 'fill-default',
    orderId: 'order-default',
    auctionId: 'auction-default',
    side: Side.BUY,
    price: '3000',
    size: '1',
    timestampUnix: 1_700_000_000n,
    ...partial,
  }
}

describe('fillsToCsv', () => {
  it('returns only the header for empty input', () => {
    expect(fillsToCsv([])).toBe(`${CSV_HEADER}\n`)
  })

  it('emits one row per fill, newest first preserved', () => {
    const fills = [
      fill({ fillId: 'a', timestampUnix: 1_700_000_100n }),
      fill({ fillId: 'b', timestampUnix: 1_700_000_000n }),
    ]
    const csv = fillsToCsv(fills)
    const lines = csv.trimEnd().split('\n')
    expect(lines[0]).toBe(CSV_HEADER)
    expect(lines).toHaveLength(3)
    expect(lines[1]).toContain('a')
    expect(lines[2]).toContain('b')
  })

  it('formats timestamps as ISO 8601 UTC', () => {
    const csv = fillsToCsv([
      fill({ timestampUnix: 1_700_000_000n }), // 2023-11-14T22:13:20Z
    ])
    expect(csv).toContain('2023-11-14T22:13:20.000Z')
  })

  it('emits the human-readable side as BUY / SELL', () => {
    const csv = fillsToCsv([fill({ side: Side.BUY }), fill({ side: Side.SELL })])
    expect(csv).toContain('BUY')
    expect(csv).toContain('SELL')
  })

  it('keeps price and size as wire-string decimals', () => {
    const csv = fillsToCsv([fill({ price: '3000.55', size: '0.1234' })])
    expect(csv).toContain('3000.55')
    expect(csv).toContain('0.1234')
  })

  it('escapes ids that contain commas or quotes', () => {
    const csv = fillsToCsv([fill({ fillId: 'a"b', auctionId: 'auc,1' })])
    expect(csv).toContain('"a""b"')
    expect(csv).toContain('"auc,1"')
  })

  it('terminates the final row with a newline', () => {
    const csv = fillsToCsv([fill({})])
    expect(csv.endsWith('\n')).toBe(true)
  })
})
