'use client'

import * as React from 'react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import type { TokenSymbol } from '@/lib/wallet/types'

import { DepositForm } from './DepositForm'

interface DepositModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialToken?: TokenSymbol
}

/**
 * Modal chrome around `<DepositForm>`. The form owns its own controller
 * and timer lifecycle: closing the modal mid-flight unmounts the form,
 * whose hook cleanup cancels the pending setTimeout. The mock has no
 * on-chain side-effect to roll back, so a cancel is graceful — we don't
 * suppress Escape / outside-click. The form is also drop-inable into the
 * F1.12 onboarding flow (#79) without pulling the Dialog primitive along.
 */
export function DepositModal({ open, onOpenChange, initialToken = 'USDC' }: DepositModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-md border border-brand-border"
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
