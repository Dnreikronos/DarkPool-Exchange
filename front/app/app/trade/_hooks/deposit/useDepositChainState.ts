'use client'

import { useCallback } from 'react'
import { erc20Abi } from 'viem'
import { useReadContracts, useWatchContractEvent } from 'wagmi'

import { config } from '@/lib/config'
import { darkPoolAbi } from '@/lib/contracts/generated'
import { useWallet } from '@/lib/wallet/hooks'
import type { Balances } from '@/lib/wallet/types'

import { formatRawBalance } from '../../_lib/balances/format-balance'

export interface DepositChainState {
  /** Per-token ERC20 allowance granted to the DarkPool, as decimal strings. */
  allowances: Balances
  /** `DarkPool.paused()` — disables deposit + withdraw when true. */
  paused: boolean
  refetch: () => void
}

const EMPTY_ALLOWANCES: Balances = { weth: '0', usdc: '0' }

/**
 * Live counterpart to the mock `useTxState`: reads the trader's ERC20
 * allowance to the DarkPool for both tokens plus the contract's paused
 * flag. `enabled` is owned by the caller (the facade) so the hook can be
 * called unconditionally (Rules of Hooks) while the underlying query and
 * watchers stay dormant under mocks or with no wallet.
 *
 * Three reads, fixed order (the mapping below relies on it):
 *   0: WETH.allowance(trader, darkPool)
 *   1: USDC.allowance(trader, darkPool)
 *   2: DarkPool.paused()
 */
export function useDepositChainState(enabled: boolean): DepositChainState {
  const { address } = useWallet()
  const addrs = config.contracts
  const ready = enabled && Boolean(address) && Boolean(addrs)

  const { data, refetch } = useReadContracts({
    allowFailure: false,
    contracts: ready
      ? ([
          {
            address: addrs!.weth,
            abi: erc20Abi,
            functionName: 'allowance',
            args: [address!, addrs!.darkPool],
          },
          {
            address: addrs!.usdc,
            abi: erc20Abi,
            functionName: 'allowance',
            args: [address!, addrs!.darkPool],
          },
          { address: addrs!.darkPool, abi: darkPoolAbi, functionName: 'paused' },
        ] as unknown as readonly never[])
      : [],
    query: { enabled: ready },
  })

  const triggerRefetch = useCallback(() => {
    void refetch()
  }, [refetch])

  // The paused banner must react to operator pause/unpause without a manual
  // refetch. Allowance freshness is owned by the controller, which refetches
  // after its own approve/deposit confirms.
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Paused',
    enabled: ready,
    onLogs: triggerRefetch,
  })
  useWatchContractEvent({
    address: addrs?.darkPool,
    abi: darkPoolAbi,
    eventName: 'Unpaused',
    enabled: ready,
    onLogs: triggerRefetch,
  })

  const tuple = data as readonly [bigint, bigint, boolean] | undefined
  return {
    allowances: tuple
      ? { weth: formatRawBalance('WETH', tuple[0]), usdc: formatRawBalance('USDC', tuple[1]) }
      : EMPTY_ALLOWANCES,
    paused: tuple ? Boolean(tuple[2]) : false,
    refetch: triggerRefetch,
  }
}
