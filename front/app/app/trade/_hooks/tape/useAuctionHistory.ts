'use client'

import { useMemo } from 'react'

import type { AuctionSummary } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { type MockStoreState, useMockStore } from '@/lib/mock-store'

export const DEFAULT_AUCTION_HISTORY_LIMIT = 50

function normalizeLimit(limit: number): number {
  return Math.max(0, Math.floor(limit))
}

/**
 * Pure selector over the mock store. Returns the newest `limit` auctions,
 * preserving the store's newest-first ordering. Exported separately so it
 * can be unit-tested without a React renderer.
 */
export function selectLatestAuctions(
  state: MockStoreState,
  limit: number = DEFAULT_AUCTION_HISTORY_LIMIT
): readonly AuctionSummary[] {
  const normalizedLimit = normalizeLimit(limit)
  if (state.recentAuctions.length <= normalizedLimit) return state.recentAuctions
  return state.recentAuctions.slice(0, normalizedLimit)
}

export interface UseAuctionHistoryOptions {
  /** Max rows to surface. Defaults to {@link DEFAULT_AUCTION_HISTORY_LIMIT}. */
  limit?: number
  /**
   * Phase 2 seam. In Phase 1 the store push beats any polling interval;
   * this is a documented no-op for the mock backend. Default 2000.
   * The REST swap (#94) will reuse this prop with setInterval.
   */
  pollMs?: number
}

/**
 * Subscribes to the mock store and returns the latest `limit` auctions.
 * The returned array is referentially stable when the underlying
 * `recentAuctions` reference is unchanged — important for `React.memo`
 * row components, which would otherwise re-render on every 1s perturb.
 */
export function useAuctionHistory(opts: UseAuctionHistoryOptions = {}): readonly AuctionSummary[] {
  const limit = normalizeLimit(opts.limit ?? DEFAULT_AUCTION_HISTORY_LIMIT)
  const recentAuctions = useMockStore((s) => s.recentAuctions)
  return useMemo(() => {
    if (recentAuctions.length <= limit) return recentAuctions
    return recentAuctions.slice(0, limit)
  }, [recentAuctions, limit])
}
