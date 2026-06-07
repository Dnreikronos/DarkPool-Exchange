'use client'

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { useAccount, useChainId, useSignMessage } from 'wagmi'
import { config } from '@/lib/config'
import type { WagmiAccountStatus } from './bridge-action'
import { computeSiweAction } from './siwe-action'
import { buildSiweMessage } from './siwe-message'
import { fetchNonce, verifySiwe } from './siwe-api'
import { clearSession, getServerSnapshot, getSnapshot, setSession, subscribe } from './session'
import type { Address } from './types'

/**
 * Drives the SIWE auth flow: after the wallet connects, sign an EIP-4361
 * message and exchange it for a session token (see
 * docs/superpowers/specs/2026-05-31-frontend-siwe-auth-design.md).
 *
 * SIWE is inert unless `config.siweEnabled` and not in mock mode — then
 * the SDK keeps using the static x-api-key and `isAuthenticated` simply
 * mirrors the wallet connection so authed UI is never gated.
 */
export interface UseSiweAuthOptions {
  /**
   * Auto-trigger sign-in on connect / account-switch. Intended for a single
   * mount point (`SiweAuthBridge`); consumers reading auth state omit it.
   */
  autoSignIn?: boolean
}

export interface UseSiweAuthReturn {
  isAuthenticated: boolean
  isAuthenticating: boolean
  address: Address | null
  error: string | null
  signIn: () => Promise<void>
  signOut: () => void
}

const SIWE_ENABLED = config.siweEnabled && !config.useMocks

// Module-level so concurrent signIn() calls (auto-trigger racing a click,
// or several mounted consumers) coalesce into one wallet prompt.
let inFlight: Promise<void> | null = null

// Bumped on every sign-out (manual or auto). An in-flight sign-in captures
// the epoch after its own clearSession() and re-checks it before storing the
// session, so a completion that races a sign-out can't resurrect the token.
let authEpoch = 0

function sameAddress(a: Address | null, b: Address | null): boolean {
  return a !== null && b !== null && a.toLowerCase() === b.toLowerCase()
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000)
}

export function useSiweAuth(options: UseSiweAuthOptions = {}): UseSiweAuthReturn {
  const { address: wagmiAddress, status } = useAccount()
  const chainId = useChainId()
  const { signMessageAsync } = useSignMessage()
  const session = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot)

  const [isAuthenticating, setIsAuthenticating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const address: Address | null = wagmiAddress ?? null
  // Live view of the connected address so an in-flight sign-in can detect a
  // disconnect / account switch that happened after it captured `address`.
  const liveAddress = useRef<Address | null>(address)
  liveAddress.current = address
  const validSessionAddress = session && session.expiresAt > nowSeconds() ? session.address : null
  const isAuthenticated = SIWE_ENABLED
    ? sameAddress(validSessionAddress, address)
    : status === 'connected'

  const signIn = useCallback(async (): Promise<void> => {
    if (!SIWE_ENABLED || !address) return
    if (inFlight) return inFlight

    const run = async (): Promise<void> => {
      setError(null)
      setIsAuthenticating(true)
      try {
        clearSession() // drop any stale token before re-authenticating
        const startEpoch = authEpoch
        const nonce = await fetchNonce(config.apiUrl)
        const message = buildSiweMessage({ address, chainId, nonce })
        const signature = await signMessageAsync({ message })
        const result = await verifySiwe(config.apiUrl, { message, signature })
        // Stale completion: the user signed out, disconnected or switched
        // accounts while the flow was in flight — drop the result instead of
        // resurrecting a session the user already invalidated.
        if (startEpoch !== authEpoch || !sameAddress(liveAddress.current, address)) return
        setSession({ token: result.token, expiresAt: result.expiresAt, address })
      } catch (cause) {
        setError((cause as Error)?.message ?? 'Sign-in failed')
      } finally {
        setIsAuthenticating(false)
      }
    }

    inFlight = run().finally(() => {
      inFlight = null
    })
    return inFlight
  }, [address, chainId, signMessageAsync])

  const signOut = useCallback((): void => {
    authEpoch += 1
    clearSession()
    setError(null)
  }, [])

  // Single auto-trigger site (the bridge). Auto sign-in fires only on an
  // address transition; a same-address session loss (expiry / 401) is a noop
  // so the user re-signs via a prompt, never a surprise wallet popup.
  const prevAddress = useRef<Address | null>(null)
  useEffect(() => {
    if (!options.autoSignIn || !SIWE_ENABLED) return
    const action = computeSiweAction(
      prevAddress.current,
      status as WagmiAccountStatus,
      address,
      validSessionAddress
    )
    if (status === 'connected' && address) prevAddress.current = address
    else if (status === 'disconnected') prevAddress.current = null

    if (action.kind === 'sign-in') {
      void signIn()
    } else if (action.kind === 'sign-out') {
      authEpoch += 1
      clearSession()
    }
  }, [options.autoSignIn, status, address, validSessionAddress, signIn])

  return { isAuthenticated, isAuthenticating, address, error, signIn, signOut }
}
