// Ladle preview surface for the My Orders panel.
//
// Each story imperatively seeds a fresh mock-store / wallet-store and
// stubs `useMyOrders` so the visual state can be inspected in isolation
// of the underlying ticker. The runtime hook is exercised by the
// auction-tick story, which renders the live store and lets the engine
// drive transitions.

import * as React from 'react'

import { create } from '@bufbuild/protobuf'
import { Toaster } from '@/components/ui/toaster'
import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'

import type { UseMyOrdersOptions, UseMyOrdersReturn } from '../../_hooks/my-orders/useMyOrders'
import type { MyOrderRow } from '../../_lib/my-orders/types'
import { MyOrdersPanel } from './MyOrdersPanel'
import { OrderBook } from '../orderbook/OrderBook'

function useConnected(connect: boolean) {
  React.useEffect(() => {
    if (connect) walletStore.connect()
    else walletStore.disconnect()
    return () => {
      walletStore.disconnect()
    }
  }, [connect])
}

const NOW_UNIX = 1700000000n

function mkOrder(overrides: Partial<OrderInfo> = {}): OrderInfo {
  return create(OrderInfoSchema, {
    id: 'o-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '1',
    remainingSize: '1',
    commitmentKey: 'mock-k',
    submittedAtUnix: NOW_UNIX,
    expiresAtUnix: 0n,
    ...overrides,
  })
}

function staticHook(rows: MyOrderRow[]): (options?: UseMyOrdersOptions) => UseMyOrdersReturn {
  return () => ({
    rows,
    userPrices: new Set(rows.filter((r) => r.status === 'open').map((r) => r.order.price)),
    cancel: () => true,
  })
}

export const Disconnected = () => {
  useConnected(false)
  return (
    <div className="bg-brand-bg p-6">
      <MyOrdersPanel useOrders={staticHook([])} />
    </div>
  )
}

export const ConnectedNoOrders = () => {
  useConnected(true)
  return (
    <div className="bg-brand-bg p-6">
      <MyOrdersPanel useOrders={staticHook([])} />
    </div>
  )
}

export const MixedStatuses = () => {
  useConnected(true)
  const rows: MyOrderRow[] = [
    {
      order: mkOrder({
        id: 'o-buy-3000',
        side: Side.BUY,
        price: '3000',
        size: '1.25',
        remainingSize: '1.25',
        submittedAtUnix: 1700000300n,
      }),
      status: 'open',
    },
    {
      order: mkOrder({
        id: 'o-buy-2995',
        side: Side.BUY,
        price: '2995',
        size: '0.5',
        remainingSize: '0.5',
        submittedAtUnix: 1700000180n,
      }),
      status: 'open',
    },
    {
      order: mkOrder({
        id: 'o-sell-3010',
        side: Side.SELL,
        price: '3010',
        size: '0.75',
        remainingSize: '0.75',
        submittedAtUnix: 1700000060n,
      }),
      status: 'filled',
    },
    {
      order: mkOrder({
        id: 'o-buy-2990',
        side: Side.BUY,
        price: '2990',
        size: '2',
        remainingSize: '2',
        submittedAtUnix: 1699999920n,
      }),
      status: 'cancelled',
    },
  ]
  return (
    <div className="bg-brand-bg p-6">
      <MyOrdersPanel useOrders={staticHook(rows)} />
      <Toaster />
    </div>
  )
}

export const OrderbookHighlightIntegration = () => {
  // Demonstrates the cross-component contract: my-orders exports a set
  // of user prices, the orderbook marks the matching rows with a 2px
  // white left edge. The Shell wires the same `userPrices` prop in
  // production once OrderBook is hoisted out of its placeholder slot.
  useConnected(true)
  const rows: MyOrderRow[] = [
    {
      order: mkOrder({ id: 'me-2995', side: Side.BUY, price: '2995', remainingSize: '1.5' }),
      status: 'open',
    },
    {
      order: mkOrder({ id: 'me-3005', side: Side.SELL, price: '3005', remainingSize: '0.8' }),
      status: 'open',
    },
  ]
  const userPrices = new Set(['2995', '3005'])
  return (
    <div className="grid h-[640px] grid-cols-[320px_minmax(0,1fr)] gap-px bg-brand-border">
      <div className="bg-brand-bg">
        <OrderBook userPrices={userPrices} refetchIntervalMs={1000} />
      </div>
      <div className="bg-brand-bg p-6">
        <MyOrdersPanel useOrders={staticHook(rows)} />
      </div>
    </div>
  )
}
