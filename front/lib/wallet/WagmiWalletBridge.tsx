'use client'

import { useEffect, useRef } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useAccount } from 'wagmi'

import { computeBridgeAction, type WagmiAccountStatus } from './bridge-action'
import { walletStore } from './mock-store'
import { clearPerTraderLocalStorage } from './per-trader-cache'
import type { Address } from './types'

/**
 * Drives `walletStore` from wagmi's connection state. Mounted once
 * inside `WalletProviders`. Tests and stories do NOT render the
 * bridge — they keep calling `walletStore.connect()` directly, so
 * this module is the only place that touches wagmi hooks. That keeps
 * the rest of the wallet surface decoupled from the connector layer.
 *
 * Cache-clear semantics (`computeBridgeAction` is the source of
 * truth and is unit-tested):
 *   - connected → disconnected: clear caches
 *   - account switch (A → B while staying connected): clear caches
 *   - first connect (null → A) or reconnect with same address: no clear
 *
 * Scope of the cache clear: only the `QueryClient` provided by
 * `WalletProviders` (the closest ancestor via context). Any descendant
 * that spins up its *own* `QueryClientProvider` — currently
 * `app/trade/_components/orderbook/OrderBook.tsx` — owns a separate
 * cache the bridge does not see. Hoisting OrderBook onto the shared
 * client is a Wave-4 follow-up; until then, scoped descendants are
 * responsible for their own per-trader invalidation.
 */
export function WagmiWalletBridge() {
  const { address, status } = useAccount()
  const queryClient = useQueryClient()
  const previousAddressRef = useRef<Address | null>(null)

  useEffect(() => {
    // wagmi types `address` as `\`0x${string}\` | undefined` which is
    // structurally identical to our local `Address`; just narrow
    // undefined → null for the reducer.
    const nextAddress: Address | null = address ?? null
    const action = computeBridgeAction(
      previousAddressRef.current,
      status as WagmiAccountStatus,
      nextAddress
    )

    if (action.kind === 'noop') return

    if (action.clearCaches) {
      queryClient.clear()
      clearPerTraderLocalStorage()
    }

    if (action.kind === 'connect') {
      walletStore.connect(action.address)
      previousAddressRef.current = action.address
    } else {
      walletStore.disconnect()
      previousAddressRef.current = null
    }
  }, [address, status, queryClient])

  return null
}
