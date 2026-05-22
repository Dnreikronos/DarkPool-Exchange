// Drives `useMyOrders` end-to-end against a real `createMockStore`
// instance, so the contract between the diffing reducer and the
// underlying zustand store is locked at the integration seam.
//
// renderToStaticMarkup runs only the initial commit (no effects), so to
// observe the hook's post-effect state we render the wrapper twice
// across mutations.

import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { createMockStore, type MockStore } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { StoreApi } from 'zustand/vanilla'

import { useMyOrders, type UseMyOrdersOptions } from './useMyOrders'

const FROZEN_NOW_SECONDS = 1_700_000_000

function freshStore(): StoreApi<MockStore> {
  return createMockStore({ seed: 7, now: () => FROZEN_NOW_SECONDS, mid: '3000', depth: 6 })
}

function Probe({ options }: { options: UseMyOrdersOptions }): JSX.Element {
  const { rows, userPrices } = useMyOrders(options)
  return (
    <div>
      <ul>
        {rows.map((r) => (
          <li key={r.order.id}>{`${r.order.id}:${r.status}:${r.order.price}`}</li>
        ))}
      </ul>
      <span data-testid="prices">{Array.from(userPrices).sort().join(',')}</span>
    </div>
  )
}

describe('useMyOrders', () => {
  it('surfaces open orders sourced from the store', () => {
    const store = freshStore()
    store.getState().placeOrder({ side: Side.BUY, price: '2995', size: '1' })
    store.getState().placeOrder({ side: Side.SELL, price: '3005', size: '0.5' })

    const html = renderToStaticMarkup(<Probe options={{ store }} />)

    expect(html).toMatch(/:open:2995/)
    expect(html).toMatch(/:open:3005/)
  })

  it('exposes the distinct set of user prices for orderbook highlighting', () => {
    const store = freshStore()
    store.getState().placeOrder({ side: Side.BUY, price: '2995', size: '1' })
    store.getState().placeOrder({ side: Side.BUY, price: '2995', size: '0.25' })
    store.getState().placeOrder({ side: Side.SELL, price: '3010', size: '0.5' })

    const html = renderToStaticMarkup(<Probe options={{ store }} />)

    expect(html).toContain('>2995,3010<')
  })

  it('drops cancelled orders from open status when the cancel() action runs', () => {
    const store = freshStore()
    const order = store.getState().placeOrder({ side: Side.BUY, price: '2995', size: '1' })

    // The hook's cancel() requires a host component. Render once to get
    // a handle into the hook (via a ref) and then call cancel().
    let cancelFn: ((id: string) => boolean) | null = null

    function Harness(): JSX.Element {
      const { cancel } = useMyOrders({ store, now: () => 1000 })
      cancelFn = cancel
      return <span />
    }
    renderToStaticMarkup(<Harness />)

    expect(cancelFn).not.toBeNull()
    const ok = cancelFn!(order.id)
    expect(ok).toBe(true)
    expect(store.getState().openOrders.find((o) => o.id === order.id)).toBeUndefined()
  })

  it('returns false when cancel() is called with an unknown id', () => {
    const store = freshStore()
    let cancelFn: ((id: string) => boolean) | null = null
    function Harness(): JSX.Element {
      const { cancel } = useMyOrders({ store, now: () => 1000 })
      cancelFn = cancel
      return <span />
    }
    renderToStaticMarkup(<Harness />)
    expect(cancelFn!('nope-not-here')).toBe(false)
  })
})
