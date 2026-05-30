// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

const TRADER = '0x4444444444444444444444444444444444444444'

vi.mock('@/lib/config', () => ({
  config: {
    useMocks: false,
    chainId: 31337,
    contracts: {
      darkPool: '0x1111111111111111111111111111111111111111',
      verifierProxy: '0x0000000000000000000000000000000000000000',
      weth: '0x2222222222222222222222222222222222222222',
      usdc: '0x3333333333333333333333333333333333333333',
    },
  },
}))

vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    address: TRADER,
    status: 'connected',
    isConnected: true,
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
}))

const refetch = vi.fn()
let capturedContracts: unknown[] = []
let watchHandlers: Array<() => void> = []

vi.mock('wagmi', () => ({
  useReadContracts: (cfg: { contracts: unknown[]; query?: { enabled?: boolean } }) => {
    capturedContracts = cfg.contracts
    return {
      data: [2_000000000000000000n, 1500_000000n, 10_500000000000000000n, 250_000000n],
      isLoading: false,
      isError: false,
      refetch,
    }
  },
  useWatchContractEvent: (cfg: { enabled?: boolean; onLogs: () => void }) => {
    if (cfg.enabled) watchHandlers.push(cfg.onLogs)
  },
}))

import { useChainBalances } from './useChainBalances'

beforeEach(() => {
  refetch.mockClear()
  capturedContracts = []
  watchHandlers = []
})

describe('useChainBalances', () => {
  it('issues a 4-call multicall and maps results to decimal strings', () => {
    const { result } = renderHook(() => useChainBalances(true))
    expect(capturedContracts).toHaveLength(4)
    expect(result.current.internal).toEqual({ weth: '2', usdc: '1500' })
    expect(result.current.wallet).toEqual({ weth: '10.5', usdc: '250' })
    expect(result.current.isLoading).toBe(false)
    expect(result.current.isError).toBe(false)
  })

  it('registers Deposit + Withdrawal watchers that refetch on log', () => {
    renderHook(() => useChainBalances(true))
    expect(watchHandlers).toHaveLength(2)
    watchHandlers[0]()
    watchHandlers[1]()
    expect(refetch).toHaveBeenCalledTimes(2)
  })
})
