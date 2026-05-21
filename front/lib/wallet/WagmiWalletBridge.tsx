'use client'

import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useAccount } from 'wagmi'

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
 * Side effects on disconnect:
 *   - the QueryClient cache is fully cleared (per-trader query keys
 *     are scoped by address; clearing all of them is the simplest and
 *     safest way to avoid leaking one account's data to the next),
 *   - per-trader localStorage entries are dropped (see
 *     `clearPerTraderLocalStorage`).
 */
export function WagmiWalletBridge() {
  const { address, status } = useAccount()
  const queryClient = useQueryClient()

  useEffect(() => {
    if (status === 'connected' && address) {
      walletStore.connect(address as Address)
      return
    }
    if (status === 'disconnected') {
      // Only emit side effects on the *transition* into disconnected
      // (walletStore.disconnect is idempotent, so the cache clear is
      // the load-bearing part of the guard).
      if (walletStore.getState().status !== 'disconnected') {
        queryClient.clear()
        clearPerTraderLocalStorage()
      }
      walletStore.disconnect()
    }
    // `reconnecting` and `connecting` are transient — we leave the
    // store in its previous state until wagmi resolves.
  }, [address, status, queryClient])

  return null
}
