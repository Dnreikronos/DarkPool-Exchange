'use client'

import { useCallback } from 'react'
import { useReadContracts, useWatchContractEvent } from 'wagmi'
import type { WatchContractEventParameters } from 'viem'

import { config } from '@/lib/config'
import { darkPoolAbi } from '@/lib/contracts/generated'
import { useWallet } from '@/lib/wallet/hooks'
import type { Balances } from '@/lib/wallet/types'

import {
  buildBalanceContracts,
  EMPTY_BALANCES,
  mapBalanceResults,
} from '../../_lib/balances/chain-reads'

export interface ChainBalances {
  wallet: Balances
  internal: Balances
  isLoading: boolean
  isError: boolean
  refetch: () => void
}

/**
 * Reads on-chain balances for the connected trader. `enabled` is owned
 * by the caller (`useBalances`) so the hook can be called
 * unconditionally (Rules of Hooks) while the underlying query/watchers
 * stay dormant under mocks or with no wallet.
 */
export function useChainBalances(enabled: boolean): ChainBalances {
  const { address } = useWallet()
  const addrs = config.contracts
  const ready = enabled && Boolean(address) && Boolean(addrs)

  const { data, isLoading, isError, refetch } = useReadContracts({
    allowFailure: false,
    // wagmi's useReadContracts generic is strict about the contracts tuple shape;
    // the heterogeneous `as const` array from buildBalanceContracts satisfies the
    // runtime contract but tsc can't unify the inferred union — cast at call site.
    contracts: ready
      ? (buildBalanceContracts(addrs!, address!) as unknown as readonly never[])
      : [],
    query: { enabled: ready },
  })

  // TanStack Query's refetch is referentially stable, so this stays stable
  // across renders and the watchers below don't tear down/re-register on
  // every parent re-render. Also the exposed `refetch`.
  const triggerRefetch = useCallback(() => {
    void refetch()
  }, [refetch])

  // wagmi narrows `args` from the abi+eventName; `trader` is the indexed
  // first param of both Deposit and Withdrawal. `satisfies` keeps the
  // object checked against the per-event args type so an ABI param rename
  // surfaces as a compile error instead of silently breaking the filter.
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Deposit',
    args: address
      ? ({ trader: address } satisfies WatchContractEventParameters<
          typeof darkPoolAbi,
          'Deposit'
        >['args'])
      : undefined,
    enabled: ready,
    onLogs: triggerRefetch,
  })
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Withdrawal',
    args: address
      ? ({ trader: address } satisfies WatchContractEventParameters<
          typeof darkPoolAbi,
          'Withdrawal'
        >['args'])
      : undefined,
    enabled: ready,
    onLogs: triggerRefetch,
  })

  const mapped = data ? mapBalanceResults(data as readonly bigint[]) : null
  return {
    wallet: mapped?.wallet ?? EMPTY_BALANCES,
    internal: mapped?.internal ?? EMPTY_BALANCES,
    isLoading,
    isError,
    refetch: triggerRefetch,
  }
}
