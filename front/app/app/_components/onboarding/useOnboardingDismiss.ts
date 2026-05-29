'use client'

import * as React from 'react'

import {
  type StorageLike,
  isDismissed as readDismissed,
  promoteAnonToAddress,
  setDismissed,
} from './storage'

export interface UseOnboardingDismissOptions {
  /** Address of the connected wallet, or null if disconnected. */
  address: string | null
  /** Injectable storage adapter — defaults to `window.localStorage`. */
  storage?: StorageLike | null
}

export interface UseOnboardingDismissReturn {
  /** True once we have read the browser storage (post-hydration). */
  isReady: boolean
  /** True iff the user has dismissed onboarding for the current bucket. */
  dismissed: boolean
  /** Persist the dismissal for the current bucket. */
  dismiss: () => void
}

function getDefaultStorage(): StorageLike | null {
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    // Safari private mode + sandboxed iframes throw on access.
    return null
  }
}

/**
 * Reads (and writes) the dismissal flag for the connected wallet. When
 * the wallet is disconnected the hook reads/writes an `anon` bucket;
 * when a wallet connects after an anon dismissal the flag is promoted
 * forward so the modal does not re-pop.
 *
 * Returns `isReady=false` during SSR and the first client commit so
 * callers can gate the modal mount on a stable client-side state and
 * avoid hydration mismatch.
 */
export function useOnboardingDismiss({
  address,
  storage,
}: UseOnboardingDismissOptions): UseOnboardingDismissReturn {
  const resolvedStorage = storage === undefined ? getDefaultStorage() : storage
  const [isReady, setIsReady] = React.useState(false)
  const [dismissed, setDismissedState] = React.useState(false)

  // Initial read after hydration so we never disagree with SSR markup.
  React.useEffect(() => {
    if (!resolvedStorage) {
      setIsReady(true)
      return
    }
    if (address) {
      promoteAnonToAddress(resolvedStorage, address)
    }
    setDismissedState(readDismissed(resolvedStorage, address))
    setIsReady(true)
  }, [resolvedStorage, address])

  const dismiss = React.useCallback(() => {
    if (!resolvedStorage) {
      setDismissedState(true)
      return
    }
    setDismissed(resolvedStorage, address)
    setDismissedState(true)
  }, [resolvedStorage, address])

  return { isReady, dismissed, dismiss }
}
