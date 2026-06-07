// Submission-time persistence hook (#101): the real pipeline calls this
// with the PlaceOrderResponse the engine returned, right after the
// `submitting` stage succeeds. Best-effort by contract — an IndexedDB
// hiccup must never fail an order the engine already accepted.

import { normalizeTraderId } from '@/lib/wallet/normalize'
import type { Address } from '@/lib/wallet/types'
import type { PlaceOrderResponse } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { getHistoryDb, type HistoryDb } from './db'
import { orderInfoToRecord } from './records'
import { recordSubmittedOrder } from './repo'

export async function persistPlacedOrder(
  resp: PlaceOrderResponse,
  traderAddress: Address | null,
  db?: HistoryDb
): Promise<void> {
  if (!resp.order || !traderAddress) return
  try {
    const trader = normalizeTraderId(traderAddress)
    await recordSubmittedOrder(db ?? getHistoryDb(), orderInfoToRecord(resp.order, trader))
  } catch {
    // Best-effort: the order is live on the engine regardless; the next
    // boot's backfill cannot recover a record we never wrote, but failing
    // the submission UX over a local-storage error would be worse.
  }
}
