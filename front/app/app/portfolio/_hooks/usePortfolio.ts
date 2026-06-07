'use client'

import { useMemo } from 'react'

import { useTraderFills } from '@/lib/history'
import { type MockStoreState, useMockStore } from '@/lib/mock-store'
import type { Fill } from '@/lib/mock-store'
import { useTraderId } from '@/lib/wallet/hooks'

import { computeDivergence, computeSummary } from '../_lib/pnl'
import type { DivergenceResult, PortfolioSummary } from '../_lib/pnl'
import type { Position } from '../_lib/pnl'

export interface UsePortfolioReturn {
  fills: readonly Fill[]
  summary: PortfolioSummary
  divergence: DivergenceResult
}

// ─── Pure selectors (unit-testable) ──────────────────────────────────────

export function selectLatestClearingPrice(state: MockStoreState): string | null {
  if (state.recentAuctions.length === 0) return null
  return state.recentAuctions[0].clearingPrice
}

// ─── React hook ─────────────────────────────────────────────────────────

/**
 * Derives the portfolio view from the PERSISTENT fill history (#101):
 * fills come from IndexedDB, scoped to the connected trader, so they
 * survive refreshes and disconnects. The mark price still reads the
 * latest mock-store auction — the auction feed is a market-wide stream,
 * not per-trader state, and its real-mode swap belongs to the tape work.
 *
 * Disconnected ⇒ no trader key ⇒ empty fills. (History is keyed per
 * address; another address on the same machine must not see it.)
 *
 * Divergence is computed against the caller-supplied `internal` balance
 * so the hook stays decoupled from the wallet store and works for both
 * the disconnected + connected paths in the panel.
 */
export function usePortfolio(internal: Position): UsePortfolioReturn {
  const trader = useTraderId()
  const fills = useTraderFills(trader)
  const mark = useMockStore(selectLatestClearingPrice)
  const summary = useMemo(() => computeSummary(fills, mark), [fills, mark])
  const divergence = useMemo(() => computeDivergence(fills, internal), [fills, internal])
  return { fills, summary, divergence }
}
