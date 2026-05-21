'use client'

import * as React from 'react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import type { TokenSymbol } from '@/lib/wallet/types'

import { WithdrawForm } from './WithdrawForm'

interface WithdrawModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialToken?: TokenSymbol
}

/**
 * Modal chrome around `<WithdrawForm>`. See `DepositModal` for the
 * cancellation rationale: the form's hook cleanup cancels the
 * setTimeout on unmount, so Esc / outside-click is intentionally not
 * suppressed.
 */
export function WithdrawModal({ open, onOpenChange, initialToken = 'USDC' }: WithdrawModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-md border border-brand-border"
        aria-labelledby="withdraw-modal-title"
      >
        <DialogTitle className="sr-only" id="withdraw-modal-title">
          Withdraw
        </DialogTitle>
        <DialogDescription className="sr-only">
          Move tokens from the DarkPool engine back to your wallet.
        </DialogDescription>
        <WithdrawForm initialToken={initialToken} onConfirmed={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}
