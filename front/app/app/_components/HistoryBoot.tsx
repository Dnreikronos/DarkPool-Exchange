'use client'

// Boot glue for the persistent fill history (#101). Renders nothing.
//
// Mock mode (placeOrder mocked): mirrors the mock store's order/fill
// transitions into Dexie under the connected trader, so the portfolio
// reads one source in both modes.
//
// Real mode: once per trader per mount, reconciles every locally-open
// order against GET /v1/orders/{id} — fills/expiries that happened while
// the tab was closed land in the history before the portfolio renders.

import { useEffect, useRef } from 'react'

import { methodOverridesFromEnv, useDarkPoolClient } from '@/lib/api-client'
import { config } from '@/lib/config'
import { backfillTrader, getHistoryDb, getOrderOrNull, startMockHistoryMirror } from '@/lib/history'
import { mockStore } from '@/lib/mock-store'
import { useTraderId } from '@/lib/wallet/hooks'

/** Mirrors OrderEntry's submission gate: where do placed orders live? */
function placeOrderIsMocked(): boolean {
  return methodOverridesFromEnv().placeOrder ?? config.useMocks
}

export function HistoryBoot(): null {
  const trader = useTraderId()
  const client = useDarkPoolClient()

  // The mirror is one long-lived subscription; it reads the live trader
  // through a ref so connect/disconnect doesn't resubscribe.
  const traderRef = useRef<string | null>(trader)
  traderRef.current = trader

  useEffect(() => {
    if (!placeOrderIsMocked()) return
    const mirror = startMockHistoryMirror(mockStore, {
      db: getHistoryDb(),
      getTrader: () => traderRef.current,
    })
    return mirror.stop
  }, [])

  // Backfill once per trader per mount (real mode only — the mock engine
  // lives in this tab, so there is nothing to catch up on).
  const backfilled = useRef<Set<string>>(new Set())
  useEffect(() => {
    if (placeOrderIsMocked() || trader === null) return
    if (backfilled.current.has(trader)) return
    backfilled.current.add(trader)
    void backfillTrader(getHistoryDb(), trader, {
      getOrder: (orderId) => getOrderOrNull(client, orderId),
      nowUnixSec: () => Math.floor(Date.now() / 1000),
    }).catch(() => {
      // Best-effort: a failed backfill retries on the next boot.
      backfilled.current.delete(trader)
    })
  }, [trader, client])

  return null
}
