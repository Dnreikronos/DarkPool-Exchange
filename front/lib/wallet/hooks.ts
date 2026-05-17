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
  connect: () => void
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
