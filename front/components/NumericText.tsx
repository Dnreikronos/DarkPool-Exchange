'use client'

import * as React from 'react'
import Decimal from 'decimal.js'

import { cn } from './ui/cn'

export type NumericKind = 'price' | 'size' | 'usd'
export type NumericAlign = 'left' | 'right'

const DEFAULT_DECIMALS: Record<NumericKind, number> = {
  price: 2,
  size: 4,
  usd: 2,
}

const THOUSANDS_THRESHOLD = 10_000

export interface NumericTextProps extends Omit<React.HTMLAttributes<HTMLSpanElement>, 'children'> {
  /**
   * Decimal value as a wire string. NumericText never accepts a JS number —
   * wire fields (`price`, `size`, `clearingPrice`, `matchedVolume`) come
   * from the API as decimal strings and must stay strings to avoid float drift.
   */
  value: string
  /** Override the per-kind default decimal count. */
  decimals?: number
  /** Defaults to `right` so columns of numbers align on the decimal point. */
  align?: NumericAlign
  /** Drives the default decimal count and is exposed as `data-kind` for future styling. */
  kind?: NumericKind
  /** Rendered when `value` is empty, non-numeric, or non-finite. */
  placeholder?: string
}

function formatDecimal(value: string, decimals: number, placeholder: string): string {
  if (typeof value !== 'string' || value.trim() === '') return placeholder
  let d: Decimal
  try {
    d = new Decimal(value)
  } catch {
    return placeholder
  }
  if (!d.isFinite()) return placeholder

  const fixed = d.toFixed(decimals)
  const isNegative = fixed.startsWith('-')
  const unsigned = isNegative ? fixed.slice(1) : fixed
  const [intPart, fracPart] = unsigned.split('.')

  const intAbs = new Decimal(intPart)
  const intRendered = intAbs.gte(THOUSANDS_THRESHOLD)
    ? intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    : intPart

  const body = fracPart !== undefined ? `${intRendered}.${fracPart}` : intRendered
  return isNegative ? `-${body}` : body
}

/**
 * Tabular, monospaced numeric span. Aligns decimal points across rows when
 * stacked in a right-aligned column with a constant `decimals` value.
 *
 * Thousands separator only kicks in at |value| ≥ 10,000 — trading screens
 * keep small numbers clean and reserve the comma for stats that need it.
 */
export const NumericText = React.forwardRef<HTMLSpanElement, NumericTextProps>(function NumericText(
  { value, decimals, align = 'right', kind = 'price', className, placeholder = '—', ...props },
  ref
) {
  const effectiveDecimals = decimals ?? DEFAULT_DECIMALS[kind]
  const formatted = formatDecimal(value, effectiveDecimals, placeholder)
  return (
    <span
      ref={ref}
      data-kind={kind}
      className={cn(
        'inline-block font-mono tabular-nums whitespace-nowrap text-brand-fg',
        align === 'right' ? 'text-right' : 'text-left',
        className
      )}
      {...props}
    >
      {formatted}
    </span>
  )
})
