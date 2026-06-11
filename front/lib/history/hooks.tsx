'use client'

// React binding for the persistent fill history (#101). Dexie's
// liveQuery re-emits whenever a touched table changes, so the portfolio
// updates the moment the mirror/backfill writes a fill — same UX as the
// old in-memory store subscription, but durable.

import { useEffect, useState } from 'react'
import { liveQuery } from 'dexie'

import type { Fill } from '@/lib/mock-store'

import { getHistoryDb } from './db'
import { fillRecordToFill } from './records'
import { listFills } from './repo'

const NO_FILLS: readonly Fill[] = []

/**
 * Live trader-scoped fills, newest first, in the `Fill` shape the
 * portfolio/CSV already consume. SSR-safe: the server commit renders the
 * empty list; the subscription starts after hydration. `trader` is the
 * normalizeTraderId form (see useTraderId); null renders empty.
 */
export function useTraderFills(trader: string | null): readonly Fill[] {
  const [fills, setFills] = useState<readonly Fill[]>(NO_FILLS)

  useEffect(() => {
    if (trader === null) {
      setFills(NO_FILLS)
      return
    }
    const subscription = liveQuery(() => listFills(getHistoryDb(), trader)).subscribe({
      next: (records) => setFills(records.map(fillRecordToFill)),
      // A broken IndexedDB (private mode, quota) degrades to an empty
      // history rather than a crashed portfolio.
      error: () => setFills(NO_FILLS),
    })
    return () => subscription.unsubscribe()
  }, [trader])

  return fills
}
