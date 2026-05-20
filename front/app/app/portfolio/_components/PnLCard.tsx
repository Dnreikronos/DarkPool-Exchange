'use client'

import * as React from 'react'

import { Decimal } from '@/lib/units'
import { NumericText } from '@/components/NumericText'

import type { PortfolioSummary } from './pnl'

export interface PnLCardProps {
  summary: PortfolioSummary
}

const WETH_DP = 4
const USDC_DP = 2

/**
 * Stat-value triplet that opens the portfolio surface: WETH position,
 * USDC cash delta, unrealized P&L vs the latest clearing price.
 *
 * Per docs/DESIGN-INSPIRATIONS.md ("/app/portfolio | The unrealized P&L
 * value (if positive)"), positive P&L is the single lime accent for
 * this view. We never use red/green semantic hues; sign comes from a
 * `+ / -` prefix in tabular mono.
 */
export function PnLCard({ summary }: PnLCardProps): JSX.Element {
  const { position, avgEntry, mark, unrealizedPnl } = summary
  const pnlSign = signOf(unrealizedPnl)
  const wethSign = signOf(position.weth)

  return (
    <section aria-label="Portfolio summary" className="border border-brand-border bg-brand-surface">
      <div className="grid grid-cols-1 divide-y divide-brand-border md:grid-cols-3 md:divide-x md:divide-y-0">
        <Stat
          label="WETH POSITION"
          value={
            <NumericText
              value={position.weth}
              decimals={WETH_DP}
              kind="size"
              align="left"
              className="text-display-sm font-display tracking-brand text-brand-fg"
            />
          }
          hint={wethSign === 'pos' ? 'LONG' : wethSign === 'neg' ? 'SHORT' : 'FLAT'}
        />
        <Stat
          label="USDC DELTA"
          value={
            <NumericText
              value={position.usdc}
              decimals={USDC_DP}
              kind="usd"
              align="left"
              className="text-display-sm font-display tracking-brand text-brand-fg"
            />
          }
          hint={avgEntry !== null ? `AVG ENTRY ${formatPriceShort(avgEntry)}` : 'NO POSITION'}
        />
        <Stat
          label="UNREALIZED P&L"
          value={
            unrealizedPnl === null ? (
              <span className="font-display text-display-sm text-brand-muted">—</span>
            ) : (
              <SignedPnl value={unrealizedPnl} sign={pnlSign} />
            )
          }
          hint={mark !== null ? `MARK ${formatPriceShort(mark)}` : 'AWAITING AUCTION'}
        />
      </div>
    </section>
  )
}

function SignedPnl({ value, sign }: { value: string; sign: Sign }) {
  const absolute = new Decimal(value).abs().toFixed(USDC_DP)
  const prefix = sign === 'pos' ? '+' : sign === 'neg' ? '−' : ''
  const cls =
    'inline-block font-display text-display-sm tracking-brand tabular-nums leading-none ' +
    (sign === 'pos' ? 'text-brand-accent' : 'text-brand-fg')
  return (
    <span className={cls} aria-label={`Unrealized P and L ${value} USDC`}>
      {prefix}
      {formatThousands(absolute)}
    </span>
  )
}

function formatThousands(decimalString: string): string {
  const [intPart, fracPart] = decimalString.split('.')
  const grouped = new Decimal(intPart).abs().gte(10000)
    ? intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    : intPart
  return fracPart !== undefined ? `${grouped}.${fracPart}` : grouped
}

function Stat({ label, value, hint }: { label: string; value: React.ReactNode; hint: string }) {
  return (
    <div className="flex flex-col gap-2 px-5 py-4">
      <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {label}
      </span>
      <div className="leading-none">{value}</div>
      <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {hint}
      </span>
    </div>
  )
}

type Sign = 'pos' | 'neg' | 'zero'

function signOf(v: string | null): Sign {
  if (v === null) return 'zero'
  const d = new Decimal(v)
  if (d.isZero()) return 'zero'
  return d.isPositive() ? 'pos' : 'neg'
}

function formatPriceShort(v: string): string {
  return new Decimal(v).toFixed(2)
}
