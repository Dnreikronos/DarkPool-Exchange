// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook } from '@testing-library/react'

const DARKPOOL = '0x1111111111111111111111111111111111111111'

const mockConfig = vi.hoisted(() => ({
  useMocks: false,
  chainId: 31337,
  contracts: {
    darkPool: '0x1111111111111111111111111111111111111111',
  } as Record<string, string> | null,
}))

vi.mock('@/lib/config', () => ({ config: mockConfig }))

interface CapturedWatch {
  address?: string
  eventName?: string
  enabled?: boolean
  onLogs: (logs: unknown[]) => void
}
let capturedWatches: CapturedWatch[] = []

vi.mock('wagmi', () => ({
  useWatchContractEvent: (cfg: CapturedWatch) => {
    capturedWatches.push(cfg)
  },
}))

import { createSettlementStore } from './store'
import { useSettlementWatch } from './useSettlementWatch'

beforeEach(() => {
  capturedWatches = []
  mockConfig.useMocks = false
  mockConfig.contracts = { darkPool: DARKPOOL }
})

const BATCH_ID = '0x00000000000000000000000000000000a1b2c3d4e5f60718293a4b5c6d7e8f90'
const TX_HASH = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

describe('useSettlementWatch', () => {
  it('subscribes to BatchSettled on the DarkPool address', () => {
    const store = createSettlementStore()
    renderHook(() => useSettlementWatch(store))
    expect(capturedWatches).toHaveLength(1)
    expect(capturedWatches[0].eventName).toBe('BatchSettled')
    expect(capturedWatches[0].address).toBe(DARKPOOL)
    expect(capturedWatches[0].enabled).toBe(true)
  })

  it('stays dormant under mocks', () => {
    mockConfig.useMocks = true
    mockConfig.contracts = null
    const store = createSettlementStore()
    renderHook(() => useSettlementWatch(store))
    expect(capturedWatches[0].enabled).toBe(false)
  })

  it('pipes decoded logs into the settlement store', () => {
    const store = createSettlementStore()
    renderHook(() => useSettlementWatch(store))
    capturedWatches[0].onLogs([
      { args: { batchId: BATCH_ID, timestamp: 1700000000n }, transactionHash: TX_HASH },
      { args: {}, transactionHash: null },
    ])
    expect(store.getState().events).toEqual([
      { batchId: BATCH_ID, txHash: TX_HASH, timestampUnix: 1700000000n },
    ])
  })
})
