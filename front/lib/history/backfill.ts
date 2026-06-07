// Boot-time backfill (#101): every order we last saw resting is checked
// against GET /v1/orders/{id} and reconciled into the local history. Runs
// once per trader per app boot (see HistoryBoot). Failures on individual
// lookups are counted and skipped — a flaky network must not wedge the
// whole history, and the next boot retries whatever stayed open.

import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/sdk/client'
import type { GetOrderRequest, GetOrderResponse } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { HistoryDb } from './db'
import { accountedFilledSize, reconcileOrder } from './reconcile'
import { applyReconciliation, fillsForOrder, listNonTerminalOrders } from './repo'

/** The slice of DarkPoolClient the backfill needs. */
export interface GetOrderClient {
  getOrder(req: Pick<GetOrderRequest, 'orderId'>): Promise<GetOrderResponse>
}

/**
 * GET /v1/orders/{id} with NOT_FOUND folded into null. The server answers
 * not_found for filled, cancelled and expired orders alike (handler.rs),
 * so for the backfill null is data, not an error.
 */
export async function getOrderOrNull(
  client: GetOrderClient,
  orderId: string
): Promise<OrderInfo | null> {
  try {
    const resp = await client.getOrder({ orderId })
    return resp.order ?? null
  } catch (err) {
    if (err instanceof DarkPoolError && err.code === DARK_POOL_ERROR_CODES.NOT_FOUND) return null
    throw err
  }
}

export interface BackfillDeps {
  /** Lookup one order: the OrderInfo, or null for not_found. */
  getOrder: (orderId: string) => Promise<OrderInfo | null>
  /** Clock in unix seconds. */
  nowUnixSec: () => number
}

export interface BackfillSummary {
  /** Non-terminal records inspected. */
  checked: number
  stillOpen: number
  filled: number
  expired: number
  /** Lookups that failed (left open; retried next boot). */
  errors: number
}

export async function backfillTrader(
  db: HistoryDb,
  trader: string,
  deps: BackfillDeps
): Promise<BackfillSummary> {
  const worklist = await listNonTerminalOrders(db, trader)
  const summary: BackfillSummary = {
    checked: 0,
    stillOpen: 0,
    filled: 0,
    expired: 0,
    errors: 0,
  }

  for (const record of worklist) {
    summary.checked += 1
    let remote: OrderInfo | null
    try {
      remote = await deps.getOrder(record.id)
    } catch {
      summary.errors += 1
      continue
    }
    const accountedFilled = accountedFilledSize(await fillsForOrder(db, record.id))
    const result = reconcileOrder({
      record,
      remote,
      accountedFilled,
      nowUnixSec: deps.nowUnixSec(),
    })
    await applyReconciliation(db, result)
    if (result.record.status === 'open') summary.stillOpen += 1
    else if (result.record.status === 'filled') summary.filled += 1
    else if (result.record.status === 'expired') summary.expired += 1
  }

  return summary
}
