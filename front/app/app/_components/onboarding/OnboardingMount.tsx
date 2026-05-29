'use client'

import * as React from 'react'
import { useAccount } from 'wagmi'

import { OnboardingDialog } from './OnboardingDialog'
import { useOnboardingDismiss } from './useOnboardingDismiss'

/**
 * Layout-level host that decides whether the onboarding modal renders.
 *
 * Rules:
 *  - The dialog opens once we have read storage (`isReady`) AND the
 *    user has not dismissed it for the current bucket (per-wallet
 *    when connected; anon otherwise).
 *  - We do not unmount on dismiss — the close animation plays on
 *    `open=false`. The dismissed flag is persisted before `open`
 *    flips so a hard reload does not see the modal again.
 *  - When the wallet later connects, the anon dismissal is promoted
 *    forward inside `useOnboardingDismiss`, so connecting does not
 *    re-trigger the modal.
 */
export function OnboardingMount() {
  const { address } = useAccount()
  const { isReady, dismissed, dismiss } = useOnboardingDismiss({
    address: address ?? null,
  })
  const [open, setOpen] = React.useState(false)

  React.useEffect(() => {
    if (!isReady) return
    if (dismissed) {
      setOpen(false)
      return
    }
    setOpen(true)
  }, [isReady, dismissed])

  const handleDismiss = React.useCallback(() => {
    dismiss()
    setOpen(false)
  }, [dismiss])

  return <OnboardingDialog open={open} onDismiss={handleDismiss} />
}
