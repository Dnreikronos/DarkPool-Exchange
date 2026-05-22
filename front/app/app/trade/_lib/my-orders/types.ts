import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

/**
 * The three states a trader-facing order can occupy in the panel.
 *
 *   - `open`:      still resting in the engine's openOrders.
 *   - `filled`:    consumed by an auction; the row lingers for the
 *                  afterlife TTL so the trader sees the fill happen.
 *   - `cancelled`: removed by the trader; the row lingers for the
 *                  afterlife TTL with an undo affordance.
 */
export type MyOrderStatus = 'open' | 'filled' | 'cancelled'

/** A row rendered by the My Orders panel. */
export interface MyOrderRow {
  order: OrderInfo
  status: MyOrderStatus
}
