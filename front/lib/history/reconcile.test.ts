import { describe, expect, it } from 'vitest'
import { create } from '@bufbuild/protobuf'

import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { accountedFilledSize, reconcileOrder } from './reconcile'
import type { FillRecord, OrderRecord } from './records'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'
const NOW = 1_700_000_300

function openRecord(overrides: Partial<OrderRecord> = {}): OrderRecord {
  return {
    id: 'order-1',
    trader: TRADER,
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '2',
    remainingSize: '2',
    status: 'open',
    commitmentKey: 'ck-1',
    submittedAtUnix: '1700000000',
    expiresAtUnix: '1700000600',
    ...overrides,
  }
}

function remoteOrder(remainingSize: string) {
  return create(OrderInfoSchema, {
    id: 'order-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '2',
    remainingSize,
    commitmentKey: 'ck-1',
    submittedAtUnix: 1_700_000_000n,
    expiresAtUnix: 1_700_000_600n,
  })
}

function fillOf(size: string, fillId = `f-${size}`): FillRecord {
  return {
    fillId,
    orderId: 'order-1',
    auctionId: 'auc-1',
    trader: TRADER,
    side: Side.BUY,
    price: '3000',
    size,
    timestampUnix: '1700000100',
  }
}

describe('accountedFilledSize', () => {
  it('sums fill sizes as a wire string', () => {
    expect(accountedFilledSize([fillOf('0.5'), fillOf('0.25')])).toBe('0.75')
  })

  it('returns 0 for no fills', () => {
    expect(accountedFilledSize([])).toBe('0')
  })

  it('never coerces through binary floats', () => {
    expect(accountedFilledSize([fillOf('0.1'), fillOf('0.2')])).toBe('0.3')
  })
})

describe('reconcileOrder · remote order still resting', () => {
  it('keeps the record open and updates remainingSize', () => {
    const { record, fill } = reconcileOrder({
      record: openRecord(),
      remote: remoteOrder('2'),
      accountedFilled: '0',
      nowUnixSec: NOW,
    })
    expect(record.status).toBe('open')
    expect(record.remainingSize).toBe('2')
    expect(fill).toBeNull()
  })

  it('synthesizes a fill for a newly-observed partial fill', () => {
    const { record, fill } = reconcileOrder({
      record: openRecord(),
      remote: remoteOrder('1.25'),
      accountedFilled: '0',
      nowUnixSec: NOW,
    })
    expect(record.status).toBe('open')
    expect(record.remainingSize).toBe('1.25')
    expect(fill).not.toBeNull()
    expect(fill!.size).toBe('0.75')
    expect(fill!.orderId).toBe('order-1')
    expect(fill!.trader).toBe(TRADER)
    expect(fill!.side).toBe(Side.BUY)
    // Clearing price is unknowable post-hoc — limit price is the documented approximation.
    expect(fill!.price).toBe('3000')
    expect(fill!.timestampUnix).toBe(String(NOW))
  })

  it('does not double-count already-recorded fills', () => {
    const { fill } = reconcileOrder({
      record: openRecord({ remainingSize: '1.25' }),
      remote: remoteOrder('1.25'),
      accountedFilled: '0.75',
      nowUnixSec: NOW,
    })
    expect(fill).toBeNull()
  })

  it('is idempotent: same inputs synthesize the same fillId', () => {
    const args = {
      record: openRecord(),
      remote: remoteOrder('1.25'),
      accountedFilled: '0',
      nowUnixSec: NOW,
    }
    expect(reconcileOrder(args).fill!.fillId).toBe(reconcileOrder(args).fill!.fillId)
  })
})

describe('reconcileOrder · remote not found (terminal)', () => {
  it('marks a not-found order past its expiry as expired, without inventing a fill', () => {
    const { record, fill } = reconcileOrder({
      record: openRecord({ expiresAtUnix: '1700000200' }),
      remote: null,
      accountedFilled: '0',
      nowUnixSec: NOW, // NOW > expiry
    })
    expect(record.status).toBe('expired')
    expect(fill).toBeNull()
  })

  it('marks a not-found order before expiry as filled and synthesizes the unaccounted remainder', () => {
    const { record, fill } = reconcileOrder({
      record: openRecord(),
      remote: null,
      accountedFilled: '0.5',
      nowUnixSec: NOW, // expiry is 1700000600, still in the future
    })
    expect(record.status).toBe('filled')
    expect(record.remainingSize).toBe('0')
    expect(fill).not.toBeNull()
    expect(fill!.size).toBe('1.5')
  })

  it('treats expiresAtUnix=0 (no expiry reported) as filled', () => {
    const { record } = reconcileOrder({
      record: openRecord({ expiresAtUnix: '0' }),
      remote: null,
      accountedFilled: '2',
      nowUnixSec: NOW,
    })
    expect(record.status).toBe('filled')
  })

  it('synthesizes no fill when fills already account for the full size', () => {
    const { fill } = reconcileOrder({
      record: openRecord(),
      remote: null,
      accountedFilled: '2',
      nowUnixSec: NOW,
    })
    expect(fill).toBeNull()
  })
})

describe('reconcileOrder · already terminal locally', () => {
  it('leaves cancelled records untouched even if the remote answer disagrees', () => {
    const cancelled = openRecord({ status: 'cancelled' })
    const { record, fill } = reconcileOrder({
      record: cancelled,
      remote: null,
      accountedFilled: '0',
      nowUnixSec: NOW,
    })
    expect(record).toEqual(cancelled)
    expect(fill).toBeNull()
  })
})
