// @vitest-environment jsdom

// Automated axe-core scan of the My Orders panel (#80) across its three
// states. See front/test/axe.ts for the rule scope.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { afterEach, describe, it } from 'vitest'
import { cleanup, render } from '@testing-library/react'

import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { OrderInfo } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'
import { expectNoAxeViolations } from '@/test/axe'

import type { UseMyOrdersReturn } from '../../_hooks/my-orders/useMyOrders'
import type { MyOrderRow } from '../../_lib/my-orders/types'
import { MyOrdersPanel } from './MyOrdersPanel'

function mkOrder(overrides: Partial<OrderInfo> = {}): OrderInfo {
  return create(OrderInfoSchema, {
    id: 'o-1',
    pair: 'ETH/USDC',
    side: Side.BUY,
    price: '3000',
    size: '1',
    remainingSize: '1',
    commitmentKey: 'mock-k',
    submittedAtUnix: 1700000000n,
    expiresAtUnix: 0n,
    ...overrides,
  })
}

function stubHook(rows: MyOrderRow[]): () => UseMyOrdersReturn {
  return () => ({
    rows,
    userPrices: new Set(rows.filter((r) => r.status === 'open').map((r) => r.order.price)),
    cancel: () => true,
  })
}

describe('MyOrdersPanel a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    render(<MyOrdersPanel useOrders={stubHook([])} />)
    await expectNoAxeViolations()
  })

  it('has no axe violations with no orders', async () => {
    walletStore.connect()
    render(<MyOrdersPanel useOrders={stubHook([])} />)
    await expectNoAxeViolations()
  })

  it('has no axe violations with open, filled and cancelled rows', async () => {
    walletStore.connect()
    render(
      <MyOrdersPanel
        useOrders={stubHook([
          { order: mkOrder({ id: 'o-open' }), status: 'open' },
          {
            order: mkOrder({ id: 'o-filled', side: Side.SELL, remainingSize: '0' }),
            status: 'filled',
          },
          { order: mkOrder({ id: 'o-cancelled' }), status: 'cancelled' },
        ])}
      />
    )
    await expectNoAxeViolations()
  })
})
