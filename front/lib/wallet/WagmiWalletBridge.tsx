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
 */
export function WagmiWalletBridge() {
  const { address, status } = useAccount()
  const queryClient = useQueryClient()
  const previousAddressRef = useRef<Address | null>(null)

  useEffect(() => {
    const nextAddress = (address ?? null) as Address | null
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
