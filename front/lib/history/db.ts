// Dexie database for the persistent fill history (#101). One database per
// browser profile, rows scoped by `trader` (normalizeTraderId form) so the
// same machine can serve several addresses without mixing histories.
//
// Lives in IndexedDB — NOT under the `dp:` localStorage namespace that
// `clearPerTraderLocalStorage` wipes on disconnect. History must survive a
// reconnect; losing it is exactly the failure mode this issue exists to
// prevent. The flip side (clearing site data erases it) is surfaced in the
// onboarding copy.

import Dexie, { type Table } from 'dexie'

import type { FillRecord, OrderRecord } from './records'

export const HISTORY_DB_NAME = 'darkpool-history'

export class HistoryDb extends Dexie {
  orders!: Table<OrderRecord, string>
  fills!: Table<FillRecord, string>

  constructor(opts: CreateHistoryDbOptions = {}) {
    super(
      opts.name ?? HISTORY_DB_NAME,
      opts.indexedDB && opts.IDBKeyRange
        ? { indexedDB: opts.indexedDB, IDBKeyRange: opts.IDBKeyRange }
        : {}
    )
    this.version(1).stores({
      // Primary key first; `[trader+status]` serves the non-terminal scan
      // the boot backfill runs, plain `trader` serves full listings.
      orders: 'id, trader, [trader+status]',
      fills: 'fillId, trader, orderId',
    })
  }
}

export interface CreateHistoryDbOptions {
  /** Override the database name (tests). */
  name?: string
  /** Inject an IndexedDB implementation (fake-indexeddb in tests). */
  indexedDB?: IDBFactory
  IDBKeyRange?: typeof IDBKeyRange
}

export function createHistoryDb(opts: CreateHistoryDbOptions = {}): HistoryDb {
  return new HistoryDb(opts)
}

let singleton: HistoryDb | null = null

/**
 * Browser-side singleton. Throws during SSR — callers are client
 * components/effects, which only run after hydration.
 */
export function getHistoryDb(): HistoryDb {
  if (typeof window === 'undefined') {
    throw new Error('getHistoryDb() is browser-only — call it from an effect or event handler.')
  }
  singleton ??= createHistoryDb()
  return singleton
}
