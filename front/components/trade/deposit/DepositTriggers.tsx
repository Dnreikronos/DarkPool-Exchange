'use client'

import * as React from 'react'

import { Button } from '../../ui/button'
import { useWallet } from '../../../lib/wallet/hooks'
import type { TokenSymbol } from '../../../lib/wallet/types'

import { DepositModal } from './DepositModal'
import { useTxState } from './hooks'
import { WithdrawModal } from './WithdrawModal'

interface DepositTriggersProps {
  /** Token to preselect when either modal opens. */
  initialToken?: TokenSymbol
  /** Compact mode: single-row buttons without surface chrome. */
  compact?: boolean
}

/**
 * Drop-in host component that pairs the deposit and withdraw modals
 * with their `[ DEPOSIT ]` / `[ WITHDRAW ]` trigger buttons. Designed
 * so any consumer (the trade shell, the balances panel header in a
 * follow-up PR, an onboarding empty state) can mount the pair in one
 * place without owning open-state plumbing.
 */
export function DepositTriggers({ initialToken = 'USDC', compact }: DepositTriggersProps) {
  const [depositOpen, setDepositOpen] = React.useState(false)
  const [withdrawOpen, setWithdrawOpen] = React.useState(false)
  const { isConnected } = useWallet()
  const tx = useTxState()
  const disabled = !isConnected || tx.paused

  return (
    <div
      className={
        compact
          ? 'flex items-stretch gap-2'
          : 'flex items-stretch gap-2 border-t border-brand-border p-3'
      }
    >
      <Button
        type="button"
        variant="primary"
        size="sm"
        className="flex-1"
        disabled={disabled}
        onClick={() => setDepositOpen(true)}
      >
        [ DEPOSIT ]
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="flex-1"
        disabled={disabled}
        onClick={() => setWithdrawOpen(true)}
      >
        [ WITHDRAW ]
      </Button>
      <DepositModal open={depositOpen} onOpenChange={setDepositOpen} initialToken={initialToken} />
      <WithdrawModal
        open={withdrawOpen}
        onOpenChange={setWithdrawOpen}
        initialToken={initialToken}
      />
    </div>
  )
}
