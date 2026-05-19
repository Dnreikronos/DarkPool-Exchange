'use client'

import * as React from 'react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../ui/dialog'
import type { TokenSymbol } from '../../../lib/wallet/types'

import { DepositForm } from './DepositForm'
import { useDepositController } from './hooks'

interface DepositModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialToken?: TokenSymbol
}

/**
 * Modal chrome around `<DepositForm>`. Kept thin so the form body is
 * also drop-inable into the F1.12 onboarding flow (#79) without
 * pulling the Dialog primitive along with it.
 */
export function DepositModal({ open, onOpenChange, initialToken = 'USDC' }: DepositModalProps) {
  // We piggyback on a separate controller instance here just to read
  // the in-flight flag for outside-click / escape suppression. The
  // form owns its own controller; both read the same global store, so
  // a confirmation propagates through both.
  const controller = useDepositController()
  const isInFlight = controller.stage.kind === 'approving' || controller.stage.kind === 'submitting'

  // We intentionally do NOT remount the form on each open. The form
  // owns its own local state — closing the modal is a soft dismissal
  // that doesn't tear down the timer; that's why the form reads from
  // its own controller. If the user reopens mid-flight, they see the
  // running stage. (This mirrors how MetaMask handles in-flight tx
  // popovers: the underlying tx isn't tied to the modal lifetime.)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-md border border-brand-border"
        onPointerDownOutside={(e) => {
          if (isInFlight) e.preventDefault()
        }}
        onEscapeKeyDown={(e) => {
          if (isInFlight) e.preventDefault()
        }}
        aria-labelledby="deposit-modal-title"
      >
        <DialogTitle className="sr-only" id="deposit-modal-title">
          Deposit
        </DialogTitle>
        <DialogDescription className="sr-only">
          Move tokens from your wallet into the DarkPool engine.
        </DialogDescription>
        <DepositForm initialToken={initialToken} onConfirmed={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}
