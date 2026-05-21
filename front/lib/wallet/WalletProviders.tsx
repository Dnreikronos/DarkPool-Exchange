'use client'

import { useState, type ReactNode } from 'react'
import '@rainbow-me/rainbowkit/styles.css'
import { RainbowKitProvider } from '@rainbow-me/rainbowkit'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { WagmiProvider } from 'wagmi'

import { darkpoolRainbowTheme } from './rainbowkit-theme'
import { getWagmiConfig } from './wagmi-config'
import { WagmiWalletBridge } from './WagmiWalletBridge'
import { WrongNetworkModal } from './WrongNetworkModal'

/**
 * Single client-side provider tree for the trading app.
 *
 *   WagmiProvider → QueryClientProvider → RainbowKitProvider
 *
 * Both wagmi and TanStack Query expect a singleton client. The
 * QueryClient is built lazily on first render so HMR in dev doesn't
 * cycle the cache, and it's the same client the WagmiWalletBridge
 * uses to clear per-trader queries on disconnect.
 */
export function WalletProviders({ children }: { children: ReactNode }) {
  // Lazy init — the function runs once per render, but the state slot
  // pins the first result for the component's lifetime.
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: false,
            refetchOnWindowFocus: false,
          },
        },
      })
  )
  const [wagmiConfig] = useState(() => getWagmiConfig())

  return (
    <WagmiProvider config={wagmiConfig}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider theme={darkpoolRainbowTheme} modalSize="compact">
          <WagmiWalletBridge />
          <WrongNetworkModal />
          {children}
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  )
}
