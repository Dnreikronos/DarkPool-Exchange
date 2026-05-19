// Pure decimal arithmetic for the order-entry preview. Everything passes
// through Decimal so the wire-string contract from docs/adr/0002-decimals.md
// is preserved end-to-end. Never use Number() or parseFloat here.

import { Decimal, toDecimal, type DecimalInput } from '../../../lib/units'

import { FEE_BPS } from './policy'

const BPS_DIVISOR = new Decimal(10_000)

/**
 * Total in quote-token units: `price * size`. Returns null if either
 * input is empty or non-numeric so the preview row can render a dash.
 */
export function computeTotal(price: string, size: string): Decimal | null {
  if (!isParseable(price) || !isParseable(size)) return null
  try {
    const p = toDecimal(price)
    const s = toDecimal(size)
    if (p.isNegative() || s.isNegative()) return null
    return p.times(s)
  } catch {
    return null
  }
}

/** Protocol fee on a given total, charged at FEE_BPS. */
export function computeFee(total: DecimalInput): Decimal {
  return toDecimal(total).times(FEE_BPS).div(BPS_DIVISOR)
}

/**
 * Grand total = total + fee. The trader pays this many quote tokens on a
 * BUY and receives `total - fee` quote tokens on a SELL. The balance
 * check (in validate.ts) consults the relevant side.
 */
export function computeGrandTotal(price: string, size: string): Decimal | null {
  const total = computeTotal(price, size)
  if (total === null) return null
  return total.plus(computeFee(total))
}

function isParseable(value: string): boolean {
  return typeof value === 'string' && value.trim() !== ''
}
