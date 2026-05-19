'use client'

import * as React from 'react'

import { NumericText } from '../../NumericText'
import type { DivergenceResult } from './pnl'

export interface DivergenceBannerProps {
  result: DivergenceResult
}

const WETH_DP = 4
const USDC_DP = 2

/**
 * Inline warning that the local fill history doesn't reconcile with the
 * internal pool balance — meaning history was lost (page refresh in
 * Phase 1; IndexedDB miss in Phase 2). The banner gives the user the
 * numeric delta so they can decide whether to keep trusting the local
 * P&L numbers.
 *
 * No semantic colors per DESIGN.md: the warning reads in `secondary`
 * text and uses bracketed-tag typography for emphasis. The 1px outline
 * is the panel boundary; we don't add a second border.
 */
export function DivergenceBanner({ result }: DivergenceBannerProps): JSX.Element | null {
  if (!result.diverged) return null

  return (
    <aside
      role="alert"
      aria-label="Position divergence warning"
      className="border border-brand-border bg-brand-surface"
    >
      <div className="flex flex-col gap-2 px-4 py-3">
        <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
          [ HISTORY OUT OF SYNC ]
        </span>
        <p className="font-mono text-body-sm text-brand-fg">
          The position derived from your local fill history doesn&apos;t match the balance the pool
          reports. Local history may have been lost — only the deltas below are visible to this
          session.
        </p>
        <dl className="grid grid-cols-[auto_minmax(0,1fr)_auto_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1 pt-1 font-mono text-body-sm text-brand-fg">
          <dt className="text-brand-muted">FROM FILLS · WETH</dt>
          <dd>
            <NumericText value={result.expected.weth} decimals={WETH_DP} kind="size" align="left" />
          </dd>
          <dt className="text-brand-muted">USDC</dt>
          <dd>
            <NumericText value={result.expected.usdc} decimals={USDC_DP} kind="usd" align="left" />
          </dd>
          <dt className="text-brand-muted">POOL · WETH</dt>
          <dd>
            <NumericText value={result.actual.weth} decimals={WETH_DP} kind="size" align="left" />
          </dd>
          <dt className="text-brand-muted">USDC</dt>
          <dd>
            <NumericText value={result.actual.usdc} decimals={USDC_DP} kind="usd" align="left" />
          </dd>
        </dl>
      </div>
    </aside>
  )
}
