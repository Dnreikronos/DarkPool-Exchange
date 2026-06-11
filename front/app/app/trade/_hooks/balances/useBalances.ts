'use client'

import { config } from '@/lib/config'
import { useInternalBalances, useWallet, useWalletBalances } from '@/lib/wallet/hooks'
import type { Balances } from '@/lib/wallet/types'

import { useChainBalances } from './useChainBalances'

export type BalancesStatus = 'disconnected' | 'loading' | 'error' | 'ready'

export interface UseBalancesResult {
  wallet: Balances
  internal: Balances
  status: BalancesStatus
  refetch: () => void
}

const EMPTY: Balances = { weth: '0', usdc: '0' }

/**
 * Per-feature mock switch, falling back to the global one. Read via
 * direct `process.env` property access so Next's NEXT_PUBLIC_* static
 * inlining still works (same constraint as lib/sdk/client.ts).
 */
function balancesUseMocks(): boolean {
  const raw = process.env.NEXT_PUBLIC_USE_MOCKS_BALANCES
  if (raw === 'true' || raw === '1') return true
  if (raw === 'false' || raw === '0') return false
  return config.useMocks
}

export function useBalances(): UseBalancesResult {
  const useMocks = balancesUseMocks()
  const { isConnected } = useWallet()

  // All hooks run unconditionally. Mock-store reads are cheap; the chain
  // query/watchers are disabled unless we're in live mode.
  const mockWallet = useWalletBalances()
  const mockInternal = useInternalBalances()
  const chain = useChainBalances(!useMocks)

  if (!isConnected) {
    return { wallet: EMPTY, internal: EMPTY, status: 'disconnected', refetch: chain.refetch }
  }
  if (useMocks) {
    return { wallet: mockWallet, internal: mockInternal, status: 'ready', refetch: () => {} }
  }
  const status: BalancesStatus = chain.isError ? 'error' : chain.isLoading ? 'loading' : 'ready'
  return { wallet: chain.wallet, internal: chain.internal, status, refetch: chain.refetch }
}
