'use client'

import { useAccount } from 'wagmi'

import { targetChain } from '@/lib/wallet'

import { ArbitrumHex } from './icons'

/**
 * Banner chip naming the chain the app is talking to.
 *
 * Before #205 this was static copy in the layout, so it claimed Arbitrum
 * on a Foundry build and "offline" on a connected wallet. It reads real
 * state now: when nothing is connected it names the chain the app was
 * built against (`NEXT_PUBLIC_CHAIN_ID`), and once a wallet is connected
 * it names the chain that wallet is actually on — including the mismatch
 * case, where reporting the target instead of the wallet's chain would
 * hide exactly the problem `WrongNetworkModal` is blocking on.
 */
export function NetworkIndicator() {
  const { isConnected, chain } = useAccount()

  // Disconnected: nothing to report but the target. Connected: report the
  // wallet's chain, which wagmi leaves undefined when it is not one of the
  // configured chains — an unrecognised chain is still a mismatch.
  const displayed = isConnected ? chain : targetChain
  const name = displayed?.name ?? 'unrecognised'
  const id = displayed?.id
  const status = !isConnected
    ? 'disconnected'
    : chain?.id === targetChain.id
      ? 'connected'
      : 'wrong network'

  return (
    <div
      role="status"
      aria-label={`Network: ${name}, chain ${id ?? 'unknown'}, status ${status}`}
      className="flex h-10 items-center gap-2 px-2"
    >
      <ArbitrumHex className="text-brand-muted" aria-hidden="true" />
      <span aria-hidden="true" className="font-mono text-label-lg uppercase text-brand-muted">
        {name} · {id ?? '—'}
      </span>
    </div>
  )
}
