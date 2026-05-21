// Pure validators for the deposit/withdraw forms.
//
// Numeric fields stay as decimal strings end-to-end. We compare via
// decimal.js so 0.1 + 0.2 doesn't drift, and the validator returns a
// branded result rather than a boolean so the modal can render the
// exact failure reason inline (the design language has no green/red,
// so failures must read in copy).

import { Decimal } from '@/lib/units'

export type ValidationOk = { ok: true; amount: Decimal }
export type ValidationErr = {
  ok: false
  reason: 'empty' | 'invalid' | 'non-positive' | 'exceeds-balance' | 'exceeds-allowance'
  message: string
}
export type ValidationResult = ValidationOk | ValidationErr

export interface ValidateDepositArgs {
  amount: string
  walletBalance: string
}

export interface ValidateWithdrawArgs {
  amount: string
  internalBalance: string
}

function parseAmount(raw: string): { ok: true; amount: Decimal } | ValidationErr {
  const trimmed = raw.trim()
  if (trimmed === '') {
    return { ok: false, reason: 'empty', message: 'Enter an amount.' }
  }
  let d: Decimal
  try {
    d = new Decimal(trimmed)
  } catch {
    return { ok: false, reason: 'invalid', message: 'Amount is not a valid number.' }
  }
  if (!d.isFinite()) {
    return { ok: false, reason: 'invalid', message: 'Amount is not a valid number.' }
  }
  if (d.lte(0)) {
    return { ok: false, reason: 'non-positive', message: 'Amount must be greater than zero.' }
  }
  return { ok: true, amount: d }
}

export function validateDeposit({ amount, walletBalance }: ValidateDepositArgs): ValidationResult {
  const parsed = parseAmount(amount)
  if (!parsed.ok) return parsed
  const wallet = new Decimal(walletBalance || '0')
  if (parsed.amount.gt(wallet)) {
    return {
      ok: false,
      reason: 'exceeds-balance',
      message: 'Insufficient wallet balance.',
    }
  }
  return parsed
}

export function validateWithdraw({
  amount,
  internalBalance,
}: ValidateWithdrawArgs): ValidationResult {
  const parsed = parseAmount(amount)
  if (!parsed.ok) return parsed
  const internal = new Decimal(internalBalance || '0')
  if (parsed.amount.gt(internal)) {
    return {
      ok: false,
      reason: 'exceeds-balance',
      message: 'Insufficient DarkPool balance.',
    }
  }
  return parsed
}

/**
 * Decide whether an approval step is needed before deposit. The mock
 * mirrors the on-chain ERC-20 contract: deposit consumes allowance and
 * an explicit `approve` is required whenever the current allowance is
 * below the requested amount.
 */
export function needsApproval(amount: string, allowance: string): boolean {
  let amt: Decimal
  let allow: Decimal
  try {
    amt = new Decimal(amount || '0')
    allow = new Decimal(allowance || '0')
  } catch {
    return true
  }
  return amt.gt(allow)
}
