// Persistent fill history (#101). Orders and fills live in IndexedDB
// (Dexie), keyed by trader, so history survives refreshes and
// disconnects. The backend has no per-trader listing endpoint (auth is a
// server-wide API key), so this client-side store IS the trader's history
// — see the follow-up issue for the post-MVP server-side endpoint.

export { backfillTrader, getOrderOrNull } from './backfill'
export type { BackfillDeps, BackfillSummary, GetOrderClient } from './backfill'
export { createHistoryDb, getHistoryDb, HISTORY_DB_NAME, HistoryDb } from './db'
export type { CreateHistoryDbOptions } from './db'
export { useTraderFills } from './hooks'
export { startMockHistoryMirror } from './mock-bridge'
export type { MockHistoryMirror, MockHistoryMirrorDeps } from './mock-bridge'
export { persistPlacedOrder } from './persist'
export { accountedFilledSize, reconcileOrder } from './reconcile'
export type { ReconcileArgs, ReconcileResult } from './reconcile'
export { fillRecordToFill, fillToFillRecord, isTerminal, orderInfoToRecord } from './records'
export type { FillRecord, OrderRecord, OrderStatus } from './records'
export {
  applyFill,
  applyReconciliation,
  fillsForOrder,
  listFills,
  listNonTerminalOrders,
  listOrders,
  markOrderCancelled,
  recordSubmittedOrder,
} from './repo'
