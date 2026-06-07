// Block-explorer URL helpers for settlement links (#100).

import type { Chain } from 'wagmi/chains'

import { targetChain } from '../wallet/wagmi-config'
import type { SettlementEvent } from './correlate'

/**
 * URL of `txHash` on the chain's default block explorer, or null when
 * the chain has none (foundry/hardhat devnets).
 */
export function txExplorerUrl(txHash: string, chain: Chain = targetChain): string | null {
  const base = chain.blockExplorers?.default?.url
  if (!base) return null
  return `${base.replace(/\/$/, '')}/tx/${txHash}`
}

/**
 * What a panel needs to render a settlement receipt: the tx hash plus
 * its explorer URL (null on explorerless devnets → render hash as text).
 */
export interface SettlementLink {
  txHash: string
  url: string | null
}

/** Projects a correlated event into the view-facing link shape. */
export function settlementLink(
  event: SettlementEvent | null | undefined,
  chain: Chain = targetChain
): SettlementLink | null {
  if (!event) return null
  return { txHash: event.txHash, url: txExplorerUrl(event.txHash, chain) }
}

/** `0xdead…beef` — head+tail truncation for tx hashes in dense rows. */
export function shortTxHash(hash: string): string {
  if (hash.length <= 12) return hash
  return `${hash.slice(0, 6)}…${hash.slice(-4)}`
}
