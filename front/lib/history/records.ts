// Plain storable record shapes for the persistent fill history (#101).
//
// IndexedDB structured-clone can hold bigints but Dexie can't index them,
// and proto Messages carry non-data fields ($typeName) — so everything is
// flattened to JSON-ish plain objects before it touches the database.
// Wire numerics (price/size/remainingSize) stay decimal STRINGS end to end
// (docs/adr/0002-decimals.md); int64 timestamps are stored as decimal
// strings and re-hydrated to bigint on the way out.
//
// `trader` is the normalizeTraderId() form (40 lowercase hex chars, no 0x)
// so every read/write keys on the same canonical spelling.

import type { OrderInfo, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'

/**
 * Lifecycle of a locally-tracked order. GET /v1/orders/{id} answers
 * not_found for filled, cancelled AND expired orders alike (deliberately
 * indistinguishable server-side), so terminal statuses are best-effort
 * inferences — see reconcile.ts.
 */
export type OrderStatus = 'open' | 'filled' | 'cancelled' | 'expired'

export interface OrderRecord {
  /** Engine-assigned order id (UUID). Primary key. */
  id: string
  /** normalizeTraderId() output — index for per-trader scoping. */
  trader: string
  pair: string
  side: Side
  price: string
  size: string
  /** Last observed unmatched portion (wire string). */
  remainingSize: string
  status: OrderStatus
  commitmentKey: string
  /** int64 seconds as decimal string. */
  submittedAtUnix: string
  /** int64 seconds as decimal string. '0' = no expiry reported. */
  expiresAtUnix: string
}

export interface FillRecord {
  /** Primary key. */
  fillId: string
  orderId: string
  auctionId: string
  trader: string
  side: Side
  price: string
  size: string
  /** int64 seconds as decimal string. */
  timestampUnix: string
}

export function isTerminal(status: OrderStatus): boolean {
  return status !== 'open'
}

/** Flatten a freshly-accepted OrderInfo into its storable record (status open). */
export function orderInfoToRecord(order: OrderInfo, trader: string): OrderRecord {
  return {
    id: order.id,
    trader,
    pair: order.pair,
    side: order.side,
    price: order.price,
    size: order.size,
    remainingSize: order.remainingSize,
    status: 'open',
    commitmentKey: order.commitmentKey,
    submittedAtUnix: order.submittedAtUnix.toString(),
    expiresAtUnix: order.expiresAtUnix.toString(),
  }
}

export function fillToFillRecord(fill: Fill, trader: string): FillRecord {
  return {
    fillId: fill.fillId,
    orderId: fill.orderId,
    auctionId: fill.auctionId,
    trader,
    side: fill.side,
    price: fill.price,
    size: fill.size,
    timestampUnix: fill.timestampUnix.toString(),
  }
}

/** Re-hydrate the consumer-facing Fill shape (portfolio, CSV export). */
export function fillRecordToFill(record: FillRecord): Fill {
  return {
    fillId: record.fillId,
    orderId: record.orderId,
    auctionId: record.auctionId,
    side: record.side,
    price: record.price,
    size: record.size,
    timestampUnix: BigInt(record.timestampUnix),
  }
}
