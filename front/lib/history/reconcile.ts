// Pure backfill reconciliation for #101. On boot every non-terminal local
// order is checked against GET /v1/orders/{id}; this module decides what
// the answer means. No I/O — the runner in backfill.ts feeds it records
// and persists what comes back.
//
// Server semantics (crates/dp-api/src/handler.rs `get_order`): an order is
// visible only while it rests in the engine. Filled, cancelled and expired
// orders all answer not_found, deliberately indistinguishable. So terminal
// statuses inferred here are best-effort:
//   - not_found  + past expiresAtUnix  → 'expired' (no fill invented)
//   - not_found  + before expiry       → 'filled'  (the engine consumed it)
//   - found      + remainingSize drop  → still 'open', partial fill observed
//
// Synthesized fills carry the order's LIMIT price — the clearing price of
// the auction that matched them is not reconstructable post-hoc (the batch
// already settled and the API keys auctions by id, not by order). For a
// buy the clearing price is ≤ limit and for a sell ≥ limit, so P&L derived
// from these fills is conservative. fillIds are deterministic in
// (orderId, accountedFilled) so re-running a backfill never duplicates.

import { Decimal } from '@/lib/units'

import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { isTerminal, type FillRecord, type OrderRecord } from './records'

export interface ReconcileArgs {
  record: OrderRecord
  /** GET /v1/orders/{id} result: the order, or null for not_found. */
  remote: OrderInfo | null
  /** Sum of fill sizes already recorded for this order (wire string). */
  accountedFilled: string
  /** Clock in unix seconds (used for expiry inference + fill timestamps). */
  nowUnixSec: number
}

export interface ReconcileResult {
  record: OrderRecord
  /** Newly-observed filled delta, or null when nothing new happened. */
  fill: FillRecord | null
}

/** Sum of recorded fill sizes, as a wire string. */
export function accountedFilledSize(fills: readonly FillRecord[]): string {
  return fills.reduce((acc, f) => acc.plus(f.size), new Decimal(0)).toFixed()
}

export function reconcileOrder(args: ReconcileArgs): ReconcileResult {
  const { record, remote, accountedFilled, nowUnixSec } = args
  if (isTerminal(record.status)) return { record, fill: null }

  if (remote !== null) {
    const filledTotal = new Decimal(record.size).minus(remote.remainingSize)
    return {
      record: { ...record, remainingSize: remote.remainingSize },
      fill: synthesizeFill(record, filledTotal, accountedFilled, nowUnixSec),
    }
  }

  const expires = record.expiresAtUnix
  const expired = expires !== '0' && new Decimal(expires).lte(nowUnixSec)
  if (expired) {
    // Whatever remained unfilled lapsed; only previously-observed fills count.
    return { record: { ...record, status: 'expired' }, fill: null }
  }

  // Gone before expiry: the engine consumed it — treat as fully filled.
  return {
    record: { ...record, status: 'filled', remainingSize: '0' },
    fill: synthesizeFill(record, new Decimal(record.size), accountedFilled, nowUnixSec),
  }
}

function synthesizeFill(
  record: OrderRecord,
  filledTotal: Decimal,
  accountedFilled: string,
  nowUnixSec: number
): FillRecord | null {
  const delta = filledTotal.minus(accountedFilled)
  if (delta.lte(0)) return null
  return {
    // Deterministic in (orderId, accountedFilled) → idempotent backfills.
    fillId: `backfill-${record.id}-${accountedFilled}`,
    orderId: record.id,
    auctionId: '',
    trader: record.trader,
    side: record.side,
    price: record.price,
    size: delta.toFixed(),
    timestampUnix: String(nowUnixSec),
  }
}
