'use client'

// Total + fee preview. Sits directly above the place button. The fee
// line carries the protocol fee in basis points so users see what
// they're paying (5 bps = 0.05% per DarkPool.sol PROTOCOL_FEE_BPS) and
// the grand total reads as a single tabular number so callers stacked
// in a column align on the decimal.

import * as React from 'react'

import { NumericText } from '../../NumericText'
import { displayDecimalsFor } from '../balances/format-balance'

import { computeFee, computeTotal } from './derive'
import { FEE_BPS, QUOTE_TOKEN } from './policy'

export interface TotalRowProps {
  price: string
  size: string
}

// Total renders at the quote-token's display precision (USDC=2dp). Fee
// renders at +2 extra dp so a 5-bps charge on a small trade like
// $5 doesn't round visibly to "0.00" — at $5 the fee is $0.0025, which
// surfaces as `0.0025 USDC`.
const TOTAL_DECIMALS = displayDecimalsFor(QUOTE_TOKEN)
const FEE_DECIMALS = TOTAL_DECIMALS + 2

export function TotalRow({ price, size }: TotalRowProps) {
  const total = computeTotal(price, size)
  const totalStr = total?.toFixed() ?? ''
  const feeStr = total ? computeFee(total).toFixed() : ''

  return (
    <dl
      aria-label="Order preview"
      className="flex flex-col gap-1 border border-brand-border bg-brand-surface px-3 py-2"
    >
      <Row
        label={`FEE · ${FEE_BPS} BPS`}
        valueNode={
          <NumericText
            value={feeStr}
            decimals={FEE_DECIMALS}
            kind="usd"
            className="text-body-sm text-brand-fg"
          />
        }
        unit={QUOTE_TOKEN}
      />
      <Row
        label={`TOTAL`}
        valueNode={
          <NumericText
            value={totalStr}
            decimals={TOTAL_DECIMALS}
            kind="usd"
            className="text-body-sm text-brand-fg"
          />
        }
        unit={QUOTE_TOKEN}
      />
    </dl>
  )
}

function Row({
  label,
  valueNode,
  unit,
}: {
  label: string
  valueNode: React.ReactNode
  unit: string
}) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <dt className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted">
        {label}
      </dt>
      <dd className="flex items-baseline gap-2">
        {valueNode}
        <span
          aria-hidden
          className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
        >
          {unit}
        </span>
      </dd>
    </div>
  )
}
