'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import type { DivergenceResult } from '../_lib/pnl'

export interface DivergenceBannerProps {
  result: DivergenceResult
}

const WETH_DP = 4
const USDC_DP = 2

/**
 * Inline warning that the local fill history can't be reconciled against
 * the pool's internal balance. The label intentionally describes the
 * symptom (mismatch) rather than the cause: in Phase 1 deposits aren't
 * tracked locally, so a non-zero pool balance OR any fill activity will
 * trigger this banner. Once #72 (deposits) and #101 (IndexedDB history)
 * land, the same banner will narrow to its intended meaning — lost local
 * history — by checking against `deposits + fills - withdrawals`.
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
      aria-label="Balance and history mismatch warning"
      className="border border-brand-border bg-brand-surface"
    >
      <div className="flex flex-col gap-2 px-4 py-3">
        <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
          [ HISTORY / BALANCE MISMATCH ]
        </span>
        <p className="font-mono text-body-sm text-brand-fg">
          Local fill history can&apos;t be reconciled against the pool balance. Phase 1 doesn&apos;t
          record on-chain deposits or withdrawals — until #72 lands, any session with fills or a
          non-zero balance surfaces here. Treat the deltas below as deltas, not as cumulative state.
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
