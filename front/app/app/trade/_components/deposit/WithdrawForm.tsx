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
  useDepositTxState,
  useWithdrawController,
  type WithdrawRevertReason,
} from '../../_hooks/deposit/hooks'
import { StepIndicator, type Step } from './StepIndicator'
import { TokenToggle } from './TokenToggle'
import { validateWithdraw } from '../../_lib/deposit/validation'

const WITHDRAW_STEPS: readonly Step[] = [{ index: '01', label: 'Withdraw' }]

const IS_DEV =
  typeof process !== 'undefined' &&
  (process.env?.NODE_ENV === 'development' || process.env?.NODE_ENV === 'test')

interface WithdrawFormProps {
  initialToken?: TokenSymbol
  onConfirmed?: () => void
  titleId?: string
}

function tokenKey(token: TokenSymbol): 'weth' | 'usdc' {
  return token === 'WETH' ? 'weth' : 'usdc'
}

export function WithdrawForm({ initialToken = 'USDC', onConfirmed, titleId }: WithdrawFormProps) {
  const [token, setToken] = React.useState<TokenSymbol>(initialToken)
  const [amount, setAmount] = React.useState('')
  const { isConnected } = useWallet()
  const { internal } = useBalances()
  const tx = useDepositTxState()
  const controller = useWithdrawController()

  React.useEffect(() => {
    if (controller.stage.kind !== 'confirmed') return
    const t = setTimeout(() => onConfirmed?.(), 800)
    return () => clearTimeout(t)
  }, [controller.stage.kind, onConfirmed])

  const tokenInternalBalance = internal[tokenKey(token)]
  const validation = validateWithdraw({
    amount,
    internalBalance: tokenInternalBalance,
  })
  const isInFlight = controller.stage.kind === 'submitting'
  const isPaused = tx.paused

  const canStart = isConnected && !isInFlight && !isPaused && validation.ok
  const formDisabled = isInFlight || controller.stage.kind === 'confirmed'

  const onMax = () => {
    if (formDisabled) return
    setAmount(tokenInternalBalance)
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
    <div className="flex flex-col gap-5" data-testid="withdraw-form">
      <header className="flex flex-col gap-2">
        <h2 id={titleId} className="font-display text-[24px] leading-none uppercase text-brand-fg">
          WITHDRAW&nbsp;—&nbsp;{token}
        </h2>
        <p className="font-mono text-body-md leading-[1.8] text-brand-muted">
          Move {token} from the DarkPool engine back to your wallet. Withdrawals settle on the next
          batch tick.
        </p>
      </header>

      <form onSubmit={onSubmit} className="flex flex-col gap-5">
        <FieldGroup label="TOKEN">
          <TokenToggle
            value={token}
            onChange={onTokenChange}
            disabled={formDisabled}
            label="Withdraw token"
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
                !validation.ok && validation.reason !== 'empty' ? 'withdraw-error' : undefined
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
              id="withdraw-error"
              className="font-mono text-label-md uppercase tracking-label text-brand-fg"
            >
              {validation.message}
            </p>
          ) : null}
        </FieldGroup>

        <BalanceRow label="DARKPOOL BALANCE" token={token} value={tokenInternalBalance} />

        <StepIndicator steps={WITHDRAW_STEPS} stage={controller.stage} currentIndex={0} />

        {isPaused ? (
          <PausedNotice />
        ) : controller.stage.kind === 'error' ? (
          <ErrorNotice message={controller.stage.errorMessage ?? 'Transaction reverted.'} />
        ) : null}

        <Button type="submit" variant="primary" disabled={!canStart} aria-busy={isInFlight}>
          {primaryLabel({
            stage: controller.stage.kind,
            phase: controller.stage.phase,
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

function BalanceRow({ label, token, value }: { label: string; token: TokenSymbol; value: string }) {
  const dp = displayDecimalsFor(token)
  return (
    <div className="flex items-baseline justify-between border-t border-brand-border pt-3">
      <span className="font-mono text-label-md uppercase tracking-label text-brand-muted">
        [ {label} ]
      </span>
      <span className="flex items-baseline gap-2">
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
        Withdrawals are suspended while the operator pauses the contract. Your DarkPool balance is
        preserved. Try again once the pause is lifted.
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
  onPick: (reason: WithdrawRevertReason) => void
}) {
  return (
    <fieldset
      className="border border-dashed border-brand-border p-3"
      aria-label="Developer: simulate revert"
    >
      <legend className="px-1 font-mono text-label-sm uppercase tracking-label text-brand-muted">
        [ DEV · SIMULATE REVERT ]
      </legend>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onPick('withdraw')}
        disabled={disabled}
      >
        [ ON WITHDRAW ]
      </Button>
    </fieldset>
  )
}

function primaryLabel({
  stage,
  phase,
  paused,
  connected,
}: {
  stage: 'idle' | 'approving' | 'submitting' | 'confirmed' | 'error'
  phase?: 'signing' | 'mining'
  paused: boolean
  connected: boolean
}): string {
  if (paused) return 'PAUSED'
  if (!connected) return 'CONNECT WALLET'
  if (stage === 'submitting' && phase === 'signing') return 'CONFIRM IN WALLET…'
  switch (stage) {
    case 'submitting':
      return 'WITHDRAWING…'
    case 'confirmed':
      return 'CONFIRMED'
    case 'error':
      return 'RETRY'
    case 'idle':
    case 'approving':
    default:
      return 'WITHDRAW'
  }
}
