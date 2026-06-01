'use client'

import * as React from 'react'

import { Button } from '@/components/ui/button'
import { cn } from '@/components/ui/cn'
import { Input } from '@/components/ui/input'
import { NumericText } from '@/components/NumericText'
import { useWallet } from '@/lib/wallet/hooks'
import type { TokenSymbol } from '@/lib/wallet/types'

import { useBalances } from '../../_hooks/balances/useBalances'
import { displayDecimalsFor } from '../../_lib/balances/format-balance'

import {
  useDepositController,
  useDepositTxState,
  type DepositRevertReason,
} from '../../_hooks/deposit/hooks'
import { StepIndicator, type Step } from './StepIndicator'
import { TokenToggle } from './TokenToggle'
import { needsApproval, validateDeposit } from '../../_lib/deposit/validation'

const DEPOSIT_STEPS: readonly Step[] = [
  { index: '01', label: 'Approve' },
  { index: '02', label: 'Deposit' },
]

const IS_DEV =
  typeof process !== 'undefined' &&
  (process.env?.NODE_ENV === 'development' || process.env?.NODE_ENV === 'test')

interface DepositFormProps {
  initialToken?: TokenSymbol
  /**
   * Called when the controller reaches the `confirmed` terminal stage
   * (after a brief flash so the user sees the success state). The modal
   * uses this to auto-close itself.
   */
  onConfirmed?: () => void
  /** Header copy override; defaults to a brutalist `[ DEPOSIT — <TOKEN> ]`. */
  titleId?: string
}

function tokenKey(token: TokenSymbol): 'weth' | 'usdc' {
  return token === 'WETH' ? 'weth' : 'usdc'
}

export function DepositForm({ initialToken = 'USDC', onConfirmed, titleId }: DepositFormProps) {
  const [token, setToken] = React.useState<TokenSymbol>(initialToken)
  const [amount, setAmount] = React.useState('')
  const { isConnected } = useWallet()
  const { wallet } = useBalances()
  const tx = useDepositTxState()
  const controller = useDepositController()

  React.useEffect(() => {
    if (controller.stage.kind !== 'confirmed') return
    const t = setTimeout(() => onConfirmed?.(), 800)
    return () => clearTimeout(t)
  }, [controller.stage.kind, onConfirmed])

  const tokenWalletBalance = wallet[tokenKey(token)]
  const tokenAllowance = tx.allowances[tokenKey(token)]
  const validation = validateDeposit({ amount, walletBalance: tokenWalletBalance })
  const requiresApproval = validation.ok ? needsApproval(amount, tokenAllowance) : true
  const isInFlight = controller.stage.kind === 'approving' || controller.stage.kind === 'submitting'
  const isPaused = controller.isPaused

  const canStart = isConnected && !isInFlight && !isPaused && validation.ok
  const formDisabled = isInFlight || controller.stage.kind === 'confirmed'

  const onMax = () => {
    if (formDisabled) return
    setAmount(tokenWalletBalance)
  }

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!canStart) return
    controller.start({ token, amount })
  }

  const onTokenChange = (next: TokenSymbol) => {
    setToken(next)
    setAmount('')
    if (controller.stage.kind === 'error') controller.reset()
  }

  return (
    <div className="flex flex-col gap-5" data-testid="deposit-form">
      <header className="flex flex-col gap-2">
        <h2 id={titleId} className="font-display text-[24px] leading-none uppercase text-brand-fg">
          DEPOSIT&nbsp;—&nbsp;{token}
        </h2>
        <p className="font-mono text-body-md leading-[1.8] text-brand-muted">
          Move {token} from your wallet into the DarkPool engine. The engine holds deposited balance
          off-chain until it is matched in a batch auction.
        </p>
      </header>

      <form onSubmit={onSubmit} className="flex flex-col gap-5">
        <FieldGroup label="TOKEN">
          <TokenToggle
            value={token}
            onChange={onTokenChange}
            disabled={formDisabled}
            label="Deposit token"
          />
        </FieldGroup>

        <FieldGroup label="AMOUNT">
          <div className="flex gap-2">
            <Input
              inputMode="decimal"
              autoComplete="off"
              placeholder="0.00"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              disabled={formDisabled}
              aria-invalid={!validation.ok && amount !== ''}
              aria-describedby={
                !validation.ok && validation.reason !== 'empty' ? 'deposit-error' : undefined
              }
              className="flex-1"
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onMax}
              disabled={formDisabled || !isConnected}
              className="h-10"
            >
              [ MAX ]
            </Button>
          </div>
          {!validation.ok && validation.reason !== 'empty' && amount !== '' ? (
            <p
              id="deposit-error"
              className="font-mono text-label-md uppercase tracking-label text-brand-fg"
            >
              {validation.message}
            </p>
          ) : null}
        </FieldGroup>

        <BalanceRow label="WALLET BALANCE" token={token} value={tokenWalletBalance} />
        <BalanceRow
          label="CURRENT ALLOWANCE"
          token={token}
          value={tokenAllowance}
          helper={requiresApproval ? '[ APPROVE REQUIRED ]' : '[ APPROVED ]'}
        />

        <StepIndicator
          steps={DEPOSIT_STEPS}
          stage={controller.stage}
          currentIndex={
            controller.stage.kind === 'submitting' || controller.stage.kind === 'confirmed' ? 1 : 0
          }
        />

        {isPaused ? (
          <PausedNotice />
        ) : controller.stage.kind === 'error' ? (
          <ErrorNotice message={controller.stage.errorMessage ?? 'Transaction reverted.'} />
        ) : null}

        <Button type="submit" variant="primary" disabled={!canStart} aria-busy={isInFlight}>
          {primaryLabel({
            stage: controller.stage.kind,
            phase: controller.stage.phase,
            requiresApproval,
            paused: isPaused,
            connected: isConnected,
          })}
        </Button>

        {IS_DEV && !isPaused ? (
          <DevRevertControls
            disabled={isInFlight || controller.stage.kind === 'confirmed'}
            onPick={(reason) => controller.simulateRevert(reason)}
          />
        ) : null}
      </form>
    </div>
  )
}

function FieldGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-2">
      <span className="font-mono text-label-md uppercase tracking-label text-brand-muted">
        [ {label} ]
      </span>
      {children}
    </label>
  )
}

function BalanceRow({
  label,
  token,
  value,
  helper,
}: {
  label: string
  token: TokenSymbol
  value: string
  helper?: string
}) {
  const dp = displayDecimalsFor(token)
  return (
    <div className="flex items-baseline justify-between border-t border-brand-border pt-3">
      <span className="font-mono text-label-md uppercase tracking-label text-brand-muted">
        [ {label} ]
      </span>
      <span className="flex items-baseline gap-2">
        {helper ? (
          <span className="font-mono text-label-sm uppercase tracking-label text-brand-muted">
            {helper}
          </span>
        ) : null}
        <NumericText
          value={value}
          decimals={dp}
          kind="size"
          aria-label={`${label} ${token}`}
          className="text-body-md"
        />
        <span className="font-mono text-label-md uppercase tracking-label text-brand-muted">
          {token}
        </span>
      </span>
    </div>
  )
}

function PausedNotice() {
  return (
    <div
      role="status"
      className={cn(
        'border border-brand-border p-3',
        'font-mono text-body-sm leading-[1.75] text-brand-fg'
      )}
    >
      <p className="font-mono text-label-md uppercase tracking-label text-brand-muted">
        [ CONTRACT PAUSED ]
      </p>
      <p className="mt-2">
        Deposits are suspended while the operator pauses the contract. The pause is used during
        emergency upgrades; balances are not affected. Try again once the pause is lifted.
      </p>
    </div>
  )
}

function ErrorNotice({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="border border-brand-border p-3 font-mono text-body-sm leading-[1.75] text-brand-fg"
    >
      <p className="font-mono text-label-md uppercase tracking-label text-brand-muted">
        [ REVERT ]
      </p>
      <p className="mt-2">{message}</p>
    </div>
  )
}

function DevRevertControls({
  disabled,
  onPick,
}: {
  disabled: boolean
  onPick: (reason: DepositRevertReason) => void
}) {
  return (
    <fieldset
      className="border border-dashed border-brand-border p-3"
      aria-label="Developer: simulate revert"
    >
      <legend className="px-1 font-mono text-label-sm uppercase tracking-label text-brand-muted">
        [ DEV · SIMULATE REVERT ]
      </legend>
      <div className="flex gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => onPick('approve')}
          disabled={disabled}
        >
          [ ON APPROVE ]
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => onPick('deposit')}
          disabled={disabled}
        >
          [ ON DEPOSIT ]
        </Button>
      </div>
    </fieldset>
  )
}

function primaryLabel({
  stage,
  phase,
  requiresApproval,
  paused,
  connected,
}: {
  stage: 'idle' | 'approving' | 'submitting' | 'confirmed' | 'error'
  phase?: 'signing' | 'mining'
  requiresApproval: boolean
  paused: boolean
  connected: boolean
}): string {
  if (paused) return 'PAUSED'
  if (!connected) return 'CONNECT WALLET'
  // `signing` is the open wallet prompt (isPending); `mining` is the tx
  // awaiting confirmation (isConfirming).
  if ((stage === 'approving' || stage === 'submitting') && phase === 'signing') {
    return 'CONFIRM IN WALLET…'
  }
  switch (stage) {
    case 'approving':
      return 'APPROVING…'
    case 'submitting':
      return 'DEPOSITING…'
    case 'confirmed':
      return 'CONFIRMED'
    case 'error':
      return 'RETRY'
    case 'idle':
    default:
      return requiresApproval ? 'APPROVE & DEPOSIT' : 'DEPOSIT'
  }
}
