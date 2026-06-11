'use client'

import { useSiweAuth } from './use-siwe-auth'

/**
 * Render-null activation point for the SIWE flow. Mounted once in
 * `WalletProviders` alongside `WagmiWalletBridge`; it is the single site
 * that auto-triggers sign-in on connect / account-switch. All of its
 * behaviour lives in the tested `useSiweAuth` hook and `computeSiweAction`
 * reducer — keeping `WagmiWalletBridge` focused on wallet-store sync.
 *
 * Inert when SIWE is disabled (mock mode / non-SIWE deployments).
 */
export function SiweAuthBridge(): null {
  useSiweAuth({ autoSignIn: true })
  return null
}
