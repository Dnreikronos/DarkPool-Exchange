'use client'

import * as React from 'react'
import { erc20Abi, parseUnits } from 'viem'
import { useConfig, useWriteContract, type Config } from 'wagmi'
import { waitForTransactionReceipt } from 'wagmi/actions'

import { config } from '@/lib/config'
import { darkPoolAbi } from '@/lib/contracts/generated'
import { TOKEN_DECIMALS } from '@/lib/units'
import { useWallet } from '@/lib/wallet/hooks'
import type { TokenSymbol } from '@/lib/wallet/types'

import { mapTxError } from '../../_lib/deposit/errors'
import { INITIAL_STAGE, reduceStage } from '../../_lib/deposit/stage-machine'
import { needsApproval } from '../../_lib/deposit/validation'
import { useDepositChainState } from './useDepositChainState'
import type { DepositController, WithdrawController } from './hooks'

function tokenAddress(token: TokenSymbol): `0x${string}` | null {
  const addrs = config.contracts
  if (!addrs) return null
  return token === 'WETH' ? addrs.weth : addrs.usdc
}

function tokenKey(token: TokenSymbol): 'weth' | 'usdc' {
  return token === 'WETH' ? 'weth' : 'usdc'
}

/**
 * Parse a human decimal amount to its raw on-chain `bigint`. Returns null
 * for anything that isn't a strictly positive number — the caller surfaces
 * that as the contract's own "zero amount" guard without spending gas.
 */
function parseAmountRaw(token: TokenSymbol, amount: string): bigint | null {
  let raw: bigint
  try {
    raw = parseUnits(amount.trim(), TOKEN_DECIMALS[token])
  } catch {
    return null
  }
  return raw > 0n ? raw : null
}

/**
 * viem's `waitForTransactionReceipt` resolves even for a mined-but-reverted
 * transaction — it reports the failure via `receipt.status`, it never throws.
 * Funnel that into the error path so the stage machine can't read a reverted
 * write as success.
 */
async function waitForSuccessfulReceipt(wagmiConfig: Config, hash: `0x${string}`): Promise<void> {
  const receipt = await waitForTransactionReceipt(wagmiConfig, { hash })
  if (receipt.status === 'reverted') {
    throw new Error('transaction reverted on-chain')
  }
}

/**
 * Live deposit controller. Mirrors the mock `DepositController` interface so
 * the form is source-agnostic. Reads the trader's allowance reactively (for
 * the approve decision + the form badge) and drives the extended stage
 * machine off wagmi's write lifecycle:
 *   writeContractAsync (signing) → hash (mining) → receipt (done).
 * Approval is for the EXACT amount — never `type(uint256).max`.
 */
export function useLiveDepositController(enabled: boolean): DepositController {
  const [stage, dispatch] = React.useReducer(reduceStage, INITIAL_STAGE)
  const { address } = useWallet()
  const wagmiConfig = useConfig()
  const { writeContractAsync } = useWriteContract()
  const chain = useDepositChainState(enabled)
  // Bumped on reset/restart; an in-flight async run checks it after every
  // await and bails if it's stale (the modal closed or the user retried).
  const runRef = React.useRef(0)
  const refetchRef = React.useRef(chain.refetch)
  refetchRef.current = chain.refetch

  const reset = React.useCallback(() => {
    runRef.current += 1
    dispatch({ type: 'reset' })
  }, [])

  const simulateRevert = React.useCallback(() => {
    // Dev affordance is mock-only; live has nothing to simulate.
  }, [])

  const allowances = chain.allowances
  const start = React.useCallback(
    ({ token, amount }: { token: TokenSymbol; amount: string }) => {
      const addr = tokenAddress(token)
      const addrs = config.contracts
      if (!addr || !addrs || !address) return

      const amountRaw = parseAmountRaw(token, amount)
      const requiresApproval = needsApproval(amount, allowances[tokenKey(token)])
      const run = (runRef.current += 1)
      const alive = () => run === runRef.current

      dispatch({ type: 'start', needsApproval: requiresApproval })

      void (async () => {
        try {
          if (amountRaw === null) {
            throw new Error('zero amount')
          }
          if (requiresApproval) {
            const hash = await writeContractAsync({
              address: addr,
              abi: erc20Abi,
              functionName: 'approve',
              args: [addrs.darkPool, amountRaw],
            })
            if (!alive()) return
            dispatch({ type: 'signed' })
            await waitForSuccessfulReceipt(wagmiConfig, hash)
            if (!alive()) return
            dispatch({ type: 'approvalDone' })
          }
          const depositHash = await writeContractAsync({
            address: addrs.darkPool,
            abi: darkPoolAbi,
            functionName: 'deposit',
            args: [addr, amountRaw],
          })
          if (!alive()) return
          dispatch({ type: 'signed' })
          await waitForSuccessfulReceipt(wagmiConfig, depositHash)
          if (!alive()) return
          dispatch({ type: 'submitted' })
          refetchRef.current()
        } catch (err) {
          if (!alive()) return
          dispatch({ type: 'fail', message: mapTxError(err).message })
        }
      })()
    },
    [address, allowances, wagmiConfig, writeContractAsync]
  )

  return { stage, isPaused: chain.paused, start, simulateRevert, reset }
}

/**
 * Live withdraw controller — single `DarkPool.withdraw(token, amount)` write,
 * no approval step.
 */
export function useLiveWithdrawController(enabled: boolean): WithdrawController {
  const [stage, dispatch] = React.useReducer(reduceStage, INITIAL_STAGE)
  const { address } = useWallet()
  const wagmiConfig = useConfig()
  const { writeContractAsync } = useWriteContract()
  const chain = useDepositChainState(enabled)
  const runRef = React.useRef(0)
  const refetchRef = React.useRef(chain.refetch)
  refetchRef.current = chain.refetch

  const reset = React.useCallback(() => {
    runRef.current += 1
    dispatch({ type: 'reset' })
  }, [])

  const simulateRevert = React.useCallback(() => {}, [])

  const start = React.useCallback(
    ({ token, amount }: { token: TokenSymbol; amount: string }) => {
      const addr = tokenAddress(token)
      const addrs = config.contracts
      if (!addr || !addrs || !address) return

      const amountRaw = parseAmountRaw(token, amount)
      const run = (runRef.current += 1)
      const alive = () => run === runRef.current

      dispatch({ type: 'start', needsApproval: false })

      void (async () => {
        try {
          if (amountRaw === null) {
            throw new Error('zero amount')
          }
          const hash = await writeContractAsync({
            address: addrs.darkPool,
            abi: darkPoolAbi,
            functionName: 'withdraw',
            args: [addr, amountRaw],
          })
          if (!alive()) return
          dispatch({ type: 'signed' })
          await waitForSuccessfulReceipt(wagmiConfig, hash)
          if (!alive()) return
          dispatch({ type: 'submitted' })
          refetchRef.current()
        } catch (err) {
          if (!alive()) return
          dispatch({ type: 'fail', message: mapTxError(err).message })
        }
      })()
    },
    [address, wagmiConfig, writeContractAsync]
  )

  return { stage, isPaused: chain.paused, start, simulateRevert, reset }
}
