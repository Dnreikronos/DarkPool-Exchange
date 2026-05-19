'use client'

import * as React from 'react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../ui/dialog'
import type { TokenSymbol } from '../../../lib/wallet/types'

import { useWithdrawController } from './hooks'
import { WithdrawForm } from './WithdrawForm'

interface WithdrawModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialToken?: TokenSymbol
}

/**
 * Modal chrome around `<WithdrawForm>`. See DepositModal for the
 * factoring rationale.
 */
export function WithdrawModal({ open, onOpenChange, initialToken = 'USDC' }: WithdrawModalProps) {
  const controller = useWithdrawController()
  const isInFlight = controller.stage.kind === 'submitting'

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
