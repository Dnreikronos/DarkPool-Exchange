// Pure validation for the order-entry form. Returns a discriminated
// result so the UI can pick which inline error to surface and the
// PlaceButton can decide whether to enable.
//
// Validation runs on every keystroke (cheap — no async, no allocations
// past a couple of Decimals) and re-runs at submit time so a stale form
// can't slip through. Errors are coded so copy lives in one place.

import { Decimal, toDecimal } from '@/lib/units'

import { computeGrandTotal } from './derive'
import { MIN_PRICE, MIN_SIZE } from './policy'

export type OrderSide = 'buy' | 'sell'

export type ValidationCode =
  | 'price-required'
  | 'price-invalid'
  | 'price-below-min'
  | 'size-required'
  | 'size-invalid'
  | 'size-below-min'
  | 'insufficient-balance'
  | 'wallet-disconnected'

export interface ValidationErrors {
  price?: ValidationCode
  size?: ValidationCode
  /** Cross-cutting issues that aren't bound to a single field. */
  form?: ValidationCode
}

export interface ValidationInput {
  side: OrderSide
  price: string
  size: string
  /** Internal (DarkPool) balance of WETH. Decimal string. */
  baseBalance: string
  /** Internal (DarkPool) balance of USDC. Decimal string. */
  quoteBalance: string
  isConnected: boolean
}

export interface ValidationResult {
  ok: boolean
  errors: ValidationErrors
}

const MIN_PRICE_DEC = new Decimal(MIN_PRICE)
const MIN_SIZE_DEC = new Decimal(MIN_SIZE)

/**
 * Validate a form snapshot. Empty strings yield required errors (not
 * `invalid`) so the user gets a stable placeholder while typing.
 *
 * Balance check semantics:
 *   - BUY  costs `price * size * (1 + fee)` in quote (USDC). The user
 *     must hold that much USDC inside the pool.
 *   - SELL costs `size` in base (WETH). The user must hold that much
 *     WETH inside the pool. The fee is collected from the quote
 *     proceeds, not from base — so the base check is purely on size.
 */
export function validateOrder(input: ValidationInput): ValidationResult {
  const errors: ValidationErrors = {}

  if (!input.isConnected) {
    errors.form = 'wallet-disconnected'
  }

  const priceCode = validateField(input.price, MIN_PRICE_DEC, {
    required: 'price-required',
    invalid: 'price-invalid',
    belowMin: 'price-below-min',
  })
  if (priceCode) errors.price = priceCode

  const sizeCode = validateField(input.size, MIN_SIZE_DEC, {
    required: 'size-required',
    invalid: 'size-invalid',
    belowMin: 'size-below-min',
  })
  if (sizeCode) errors.size = sizeCode

  if (!errors.price && !errors.size) {
    if (!checkBalance(input)) {
      errors.form = errors.form ?? 'insufficient-balance'
    }
  }

  return { ok: Object.keys(errors).length === 0, errors }
}

function validateField(
  raw: string,
  min: Decimal,
  codes: { required: ValidationCode; invalid: ValidationCode; belowMin: ValidationCode }
): ValidationCode | undefined {
  if (typeof raw !== 'string' || raw.trim() === '') return codes.required
  let d: Decimal
  try {
    d = toDecimal(raw)
  } catch {
    return codes.invalid
  }
  if (!d.isFinite() || d.isNegative() || d.isZero()) return codes.invalid
  if (d.lt(min)) return codes.belowMin
  return undefined
}

function checkBalance(input: ValidationInput): boolean {
  // Skip the balance check when the wallet is disconnected — that case
  // is already represented by `wallet-disconnected` and we don't want
  // a misleading "insufficient balance" error stacking on top.
  if (!input.isConnected) return true

  if (input.side === 'buy') {
    const grand = computeGrandTotal(input.price, input.size)
    if (grand === null) return true
    return safeLte(grand, input.quoteBalance)
  }
  // sell
  let size: Decimal
  try {
    size = toDecimal(input.size)
  } catch {
    return true
  }
  return safeLte(size, input.baseBalance)
}

function safeLte(value: Decimal, balanceRaw: string): boolean {
  let balance: Decimal
  try {
    balance = toDecimal(balanceRaw)
  } catch {
    return false
  }
  return value.lte(balance)
}
