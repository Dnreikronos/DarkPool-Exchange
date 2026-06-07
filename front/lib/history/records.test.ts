import { describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'

import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'

import {
  fillRecordToFill,
  fillToFillRecord,
  isTerminal,
  orderInfoToRecord,
  type FillRecord,
  type OrderRecord,
} from './records'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'

function sampleOrderInfo() {
  return create(OrderInfoSchema, {
    id: 'order-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000.12',
    size: '1.5',
    remainingSize: '0.5',
    commitmentKey: 'ck-1',
    submittedAtUnix: 1_700_000_000n,
    expiresAtUnix: 1_700_000_600n,
  })
}

describe('isTerminal', () => {
  it('treats open as non-terminal', () => {
    expect(isTerminal('open')).toBe(false)
  })

  it('treats filled / cancelled / expired as terminal', () => {
    expect(isTerminal('filled')).toBe(true)
    expect(isTerminal('cancelled')).toBe(true)
    expect(isTerminal('expired')).toBe(true)
  })
})

describe('orderInfoToRecord', () => {
  it('maps an OrderInfo onto a plain record keyed by trader, status open', () => {
    const record = orderInfoToRecord(sampleOrderInfo(), TRADER)
    expect(record).toEqual<OrderRecord>({
      id: 'order-1',
      trader: TRADER,
      pair: 'ETH/USDC',
      side: Side.BUY,
      price: '3000.12',
      size: '1.5',
      remainingSize: '0.5',
      status: 'open',
      commitmentKey: 'ck-1',
      submittedAtUnix: '1700000000',
      expiresAtUnix: '1700000600',
    })
  })

  it('keeps wire numerics as strings (never JS numbers)', () => {
    const record = orderInfoToRecord(sampleOrderInfo(), TRADER)
    expect(typeof record.price).toBe('string')
    expect(typeof record.size).toBe('string')
    expect(typeof record.remainingSize).toBe('string')
    expect(typeof record.submittedAtUnix).toBe('string')
    expect(typeof record.expiresAtUnix).toBe('string')
  })
})

describe('fillToFillRecord / fillRecordToFill', () => {
  const fill: Fill = {
    fillId: 'fill-1',
    orderId: 'order-1',
    auctionId: 'auc-1',
    side: Side.SELL,
    price: '2999.5',
    size: '0.25',
    timestampUnix: 1_700_000_123n,
  }

  it('round-trips a Fill through the storable record shape', () => {
    const record = fillToFillRecord(fill, TRADER)
    expect(record).toEqual<FillRecord>({
      fillId: 'fill-1',
      orderId: 'order-1',
      auctionId: 'auc-1',
      trader: TRADER,
      side: Side.SELL,
      price: '2999.5',
      size: '0.25',
      timestampUnix: '1700000123',
    })
    expect(fillRecordToFill(record)).toEqual(fill)
  })

  it('stores the timestamp as a decimal string so IndexedDB never sees a bigint', () => {
    const record = fillToFillRecord(fill, TRADER)
    expect(record.timestampUnix).toBe('1700000123')
  })
})
