'use client'

import { useCallback, useSyncExternalStore } from 'react'
import { walletStore } from './mock-store'
import { normalizeTraderId } from './normalize'
import type { Address, Balances, WalletState, WalletStatus } from './types'

function useWalletState(): WalletState {
  return useSyncExternalStore(walletStore.subscribe, walletStore.getState, walletStore.getState)
}

export interface UseWalletReturn {
  address: Address | null
  status: WalletStatus
  isConnected: boolean
  isConnecting: boolean
  /**
   * **Mock/test-only.** Calls `walletStore.connect()` synchronously
   * with `MOCK_ADDRESS`. In production the connection is owned by
   * RainbowKit + `WagmiWalletBridge`; calling this from real-app code
   * desyncs the store from wagmi for one tick before the bridge
   * overwrites it. New code should open RainbowKit's connect modal
   * (e.g. via `ConnectButton.Custom` or `useConnectModal()`) instead.
   */
  connect: () => void
  /**
   * **Mock/test-only.** Calls `walletStore.disconnect()` synchronously.
   * In production the connection is owned by RainbowKit + the bridge;
   * new code should call wagmi's `useDisconnect()` directly so the
   * bridge sees the transition and clears per-trader caches.
   */
  disconnect: () => void
}

export function useWallet(): UseWalletReturn {
  const state = useWalletState()
  const connect = useCallback(() => walletStore.connect(), [])
  const disconnect = useCallback(() => walletStore.disconnect(), [])
  return {
    address: state.address,
    status: state.status,
    isConnected: state.status === 'connected',
    isConnecting: state.status === 'connecting',
    connect,
    disconnect,
  }
}

export function useWalletBalances(): Balances {
  return useWalletState().walletBalances
}

export function useInternalBalances(): Balances {
  return useWalletState().internalBalances
}

export function useTraderId(): string | null {
  const { address } = useWalletState()
  return address ? normalizeTraderId(address) : null
}
