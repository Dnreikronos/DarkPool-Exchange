// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook } from '@testing-library/react'

vi.mock('@/lib/config', () => ({ config: { useMocks: true, contracts: null, chainId: 31337 } }))

let connected = true
vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    isConnected: connected,
    address: connected ? '0x4444444444444444444444444444444444444444' : null,
    status: connected ? 'connected' : 'disconnected',
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
  useWalletBalances: () => ({ weth: '1', usdc: '1000' }),
  useInternalBalances: () => ({ weth: '0', usdc: '0' }),
}))

const chain = {
  wallet: { weth: '2', usdc: '1500' },
  internal: { weth: '0.5', usdc: '50' },
  isLoading: false,
  isError: false,
  refetch: vi.fn(),
}
vi.mock('./useChainBalances', () => ({ useChainBalances: () => chain }))

import { useBalances } from './useBalances'

beforeEach(() => {
  connected = true
})
afterEach(() => {
  vi.unstubAllEnvs()
})

describe('useBalances', () => {
  it('returns mock balances when the global mock switch is on', () => {
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('ready')
    expect(result.current.wallet).toEqual({ weth: '1', usdc: '1000' })
    expect(result.current.internal).toEqual({ weth: '0', usdc: '0' })
  })

  it('returns disconnected status when no wallet is connected', () => {
    connected = false
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('disconnected')
  })

  it('uses chain balances when NEXT_PUBLIC_USE_MOCKS_BALANCES=false', () => {
    vi.stubEnv('NEXT_PUBLIC_USE_MOCKS_BALANCES', 'false')
    const { result } = renderHook(() => useBalances())
    expect(result.current.status).toBe('ready')
    expect(result.current.wallet).toEqual({ weth: '2', usdc: '1500' })
    expect(result.current.internal).toEqual({ weth: '0.5', usdc: '50' })
  })
})
