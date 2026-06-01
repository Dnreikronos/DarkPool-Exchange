// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

const TRADER = '0x4444444444444444444444444444444444444444'
const DARKPOOL = '0x1111111111111111111111111111111111111111'
const WETH = '0x2222222222222222222222222222222222222222'
const USDC = '0x3333333333333333333333333333333333333333'

vi.mock('@/lib/config', () => ({
  config: {
    useMocks: false,
    chainId: 31337,
    contracts: {
      darkPool: '0x1111111111111111111111111111111111111111',
      verifierProxy: '0x0',
      weth: '0x2222222222222222222222222222222222222222',
      usdc: '0x3333333333333333333333333333333333333333',
    },
  },
}))

vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    address: '0x4444444444444444444444444444444444444444',
    status: 'connected',
    isConnected: true,
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
}))

const refetch = vi.fn()
let capturedContracts: Array<{ functionName: string; address: string; args?: unknown[] }> = []
let queryEnabled: boolean | undefined
let watched: Array<{ eventName: string; enabled?: boolean; onLogs: () => void }> = []

vi.mock('wagmi', () => ({
  useReadContracts: (cfg: {
    contracts: Array<{ functionName: string; address: string; args?: unknown[] }>
    query?: { enabled?: boolean }
  }) => {
    capturedContracts = cfg.contracts
    queryEnabled = cfg.query?.enabled
    return {
      // allowance WETH = 2e18, allowance USDC = 1500e6, paused = true.
      // wagmi returns undefined data while the query is disabled.
      data: cfg.query?.enabled ? [2_000000000000000000n, 1500_000000n, true] : undefined,
      isLoading: false,
      isError: false,
      refetch,
    }
  },
  useWatchContractEvent: (cfg: { eventName: string; enabled?: boolean; onLogs: () => void }) => {
    if (cfg.enabled) watched.push(cfg)
  },
}))

import { useDepositChainState } from './useDepositChainState'

beforeEach(() => {
  refetch.mockClear()
  capturedContracts = []
  queryEnabled = undefined
  watched = []
})

describe('useDepositChainState', () => {
  it('reads allowance(trader,darkPool) for both tokens + paused()', () => {
    renderHook(() => useDepositChainState(true))
    expect(capturedContracts).toHaveLength(3)
    expect(capturedContracts[0]).toMatchObject({
      address: WETH,
      functionName: 'allowance',
      args: [TRADER, DARKPOOL],
    })
    expect(capturedContracts[1]).toMatchObject({
      address: USDC,
      functionName: 'allowance',
      args: [TRADER, DARKPOOL],
    })
    expect(capturedContracts[2]).toMatchObject({ address: DARKPOOL, functionName: 'paused' })
  })

  it('maps raw allowances to decimal strings and surfaces paused', () => {
    const { result } = renderHook(() => useDepositChainState(true))
    expect(result.current.allowances).toEqual({ weth: '2', usdc: '1500' })
    expect(result.current.paused).toBe(true)
  })

  it('registers Paused + Unpaused watchers that refetch on log', () => {
    renderHook(() => useDepositChainState(true))
    const names = watched.map((w) => w.eventName).sort()
    expect(names).toEqual(['Paused', 'Unpaused'])
    watched.forEach((w) => w.onLogs())
    expect(refetch).toHaveBeenCalledTimes(2)
  })

  it('stays dormant when disabled: query disabled, empty allowances, not paused', () => {
    queryEnabled = undefined
    const { result } = renderHook(() => useDepositChainState(false))
    expect(queryEnabled).toBe(false)
    expect(result.current.allowances).toEqual({ weth: '0', usdc: '0' })
    expect(result.current.paused).toBe(false)
    expect(watched).toHaveLength(0)
  })
})
