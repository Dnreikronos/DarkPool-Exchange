'use client'

import { useMemo } from 'react'

import { type MockStoreState, useMockStore } from '@/lib/mock-store'
import type { Fill } from '@/lib/mock-store'

import { computeDivergence, computeSummary } from '../_lib/pnl'
import type { DivergenceResult, PortfolioSummary } from '../_lib/pnl'
import type { Position } from '../_lib/pnl'

export interface UsePortfolioReturn {
  fills: readonly Fill[]
  summary: PortfolioSummary
  divergence: DivergenceResult
}

// ─── Pure selectors (unit-testable) ──────────────────────────────────────

export function selectPortfolioFills(state: MockStoreState): readonly Fill[] {
  return state.fillHistory
}

export function selectLatestClearingPrice(state: MockStoreState): string | null {
  if (state.recentAuctions.length === 0) return null
  return state.recentAuctions[0].clearingPrice
}

export function selectPortfolioSummary(state: MockStoreState): PortfolioSummary {
  return computeSummary(selectPortfolioFills(state), selectLatestClearingPrice(state))
}

// ─── React hook ─────────────────────────────────────────────────────────

/**
 * Subscribes to the mock store and derives the portfolio view. The summary
 * is memoised on the underlying `fillHistory` + `recentAuctions[0]`
 * references — both stable when the store hasn't pushed a new auction —
 * so the panel only re-renders when something actually changed.
 *
 * Divergence is computed against the caller-supplied `internal` balance
 * so the hook stays decoupled from the wallet store and works for both
 * the disconnected + connected paths in the panel.
 */
export function usePortfolio(internal: Position): UsePortfolioReturn {
  const fills = useMockStore(selectPortfolioFills)
  const mark = useMockStore(selectLatestClearingPrice)
  const summary = useMemo(() => computeSummary(fills, mark), [fills, mark])
  const divergence = useMemo(() => computeDivergence(fills, internal), [fills, internal])
  return { fills, summary, divergence }
}
