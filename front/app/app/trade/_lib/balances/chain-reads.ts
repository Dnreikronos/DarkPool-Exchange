import { erc20Abi } from 'viem'

import { darkPoolAbi } from '@/lib/contracts/generated'
import type { Address, Balances } from '@/lib/wallet/types'

import { formatRawBalance } from './format-balance'

export interface BalanceAddresses {
  darkPool: Address
  weth: Address
  usdc: Address
}

export const EMPTY_BALANCES: Balances = { weth: '0', usdc: '0' }

/**
 * The four reads, in a fixed order that `mapBalanceResults` relies on:
 *   0: DarkPool.balances(trader, WETH)  -> internal WETH
 *   1: DarkPool.balances(trader, USDC)  -> internal USDC
 *   2: WETH.balanceOf(trader)           -> wallet WETH
 *   3: USDC.balanceOf(trader)           -> wallet USDC
 * Returned shaped for wagmi's `useReadContracts`.
 */
export function buildBalanceContracts(addrs: BalanceAddresses, trader: Address) {
  return [
    {
      address: addrs.darkPool,
      abi: darkPoolAbi,
      functionName: 'balances',
      args: [trader, addrs.weth],
    },
    {
      address: addrs.darkPool,
      abi: darkPoolAbi,
      functionName: 'balances',
      args: [trader, addrs.usdc],
    },
    { address: addrs.weth, abi: erc20Abi, functionName: 'balanceOf', args: [trader] },
    { address: addrs.usdc, abi: erc20Abi, functionName: 'balanceOf', args: [trader] },
  ] as const
}

export function mapBalanceResults(results: readonly bigint[]): {
  wallet: Balances
  internal: Balances
} {
  const [internalWeth, internalUsdc, walletWeth, walletUsdc] = results
  return {
    internal: {
      weth: formatRawBalance('WETH', internalWeth),
      usdc: formatRawBalance('USDC', internalUsdc),
    },
    wallet: {
      weth: formatRawBalance('WETH', walletWeth),
      usdc: formatRawBalance('USDC', walletUsdc),
    },
  }
}
