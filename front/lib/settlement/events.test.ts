import { describe, expect, it } from 'vitest'

import { settlementEventsFromLogs, type BatchSettledLog } from './events'

const BATCH_ID = '0x00000000000000000000000000000000a1b2c3d4e5f60718293a4b5c6d7e8f90'
const TX_HASH = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

function log(overrides: Partial<BatchSettledLog> = {}): BatchSettledLog {
  return {
    args: { batchId: BATCH_ID, timestamp: 1700000000n },
    transactionHash: TX_HASH,
    ...overrides,
  }
}

describe('settlementEventsFromLogs', () => {
  it('maps a decoded BatchSettled log to a SettlementEvent', () => {
    expect(settlementEventsFromLogs([log()])).toEqual([
      { batchId: BATCH_ID, txHash: TX_HASH, timestampUnix: 1700000000n },
    ])
  })

  it('skips pending logs without a transaction hash', () => {
    expect(settlementEventsFromLogs([log({ transactionHash: null })])).toEqual([])
  })

  it('skips logs whose args failed to decode', () => {
    expect(settlementEventsFromLogs([log({ args: {} })])).toEqual([])
    expect(settlementEventsFromLogs([log({ args: { batchId: BATCH_ID } })])).toEqual([])
    expect(settlementEventsFromLogs([log({ args: { timestamp: 1n } })])).toEqual([])
  })

  it('keeps valid logs when mixed with invalid ones', () => {
    const out = settlementEventsFromLogs([log({ transactionHash: null }), log()])
    expect(out).toHaveLength(1)
    expect(out[0].txHash).toBe(TX_HASH)
  })
})
