import { create } from '@bufbuild/protobuf'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { Decimal } from '../../units'
import { createMockStore } from '../../mock-store'

import {
  CancelOrderRequestSchema,
  GetAuctionHistoryRequestSchema,
  GetOrderRequestSchema,
  PlaceOrderRequestSchema,
  Side,
  StreamAuctionsRequestSchema,
} from '../proto/darkpool/v1/darkpool_pb.js'

import { DARK_POOL_ERROR_CODES } from '../client'
import { StoreMockClient, withMockPayload } from './client'

const FROZEN_NOW = 1700000000

function freshClient(seed = 42) {
  const store = createMockStore({ seed, now: () => FROZEN_NOW, mid: '3000', depth: 6 })
  return { store, client: new StoreMockClient({ store }) }
}

describe('StoreMockClient.placeOrder', () => {
  it('drops the order into the store when payload metadata is attached', async () => {
    const { store, client } = freshClient()
    const req = withMockPayload(
      create(PlaceOrderRequestSchema, {
        commitment: new Uint8Array([0xaa, 0xbb, 0xcc, 0xdd]),
      }),
      { side: Side.BUY, price: '2998', size: '0.5' }
    )
    const resp = await client.placeOrder(req)
    expect(resp.order?.side).toBe(Side.BUY)
    expect(resp.order?.price).toBe('2998')
    expect(resp.order?.size).toBe('0.5')
    expect(store.getState().openOrders[0].id).toBe(resp.order?.id)
    expect(resp.order?.commitmentKey).toBe('mock-commitment-aabbccdd')
  })

  it('throws INVALID_ARGUMENT without payload metadata', async () => {
    const { client } = freshClient()
    await expect(
      client.placeOrder(create(PlaceOrderRequestSchema, { commitment: new Uint8Array([1]) }))
    ).rejects.toMatchObject({
      name: 'DarkPoolError',
      code: DARK_POOL_ERROR_CODES.INVALID_ARGUMENT,
    })
  })

  it('rejects SIDE_UNSPECIFIED', async () => {
    const { client } = freshClient()
    const req = withMockPayload(create(PlaceOrderRequestSchema), {
      side: Side.UNSPECIFIED,
      price: '3000',
      size: '1',
    })
    await expect(client.placeOrder(req)).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.INVALID_ARGUMENT,
    })
  })
})

describe('StoreMockClient.cancelOrder', () => {
  it('removes the order through the store', async () => {
    const { store, client } = freshClient()
    const placed = store.getState().placeOrder({ side: Side.SELL, price: '3005', size: '1' })
    await client.cancelOrder(create(CancelOrderRequestSchema, { orderId: placed.id }))
    expect(store.getState().openOrders.some((o) => o.id === placed.id)).toBe(false)
  })

  it('throws NOT_FOUND on unknown ids', async () => {
    const { client } = freshClient()
    await expect(
      client.cancelOrder(create(CancelOrderRequestSchema, { orderId: 'ghost' }))
    ).rejects.toMatchObject({ code: DARK_POOL_ERROR_CODES.NOT_FOUND })
  })
})

describe('StoreMockClient.getOrder', () => {
  it('returns an order that was placed via the store', async () => {
    const { store, client } = freshClient()
    const placed = store.getState().placeOrder({ side: Side.BUY, price: '2990', size: '0.25' })
    const resp = await client.getOrder(create(GetOrderRequestSchema, { orderId: placed.id }))
    expect(resp.order?.id).toBe(placed.id)
  })

  it('throws NOT_FOUND when missing', async () => {
    const { client } = freshClient()
    await expect(
      client.getOrder(create(GetOrderRequestSchema, { orderId: 'ghost' }))
    ).rejects.toMatchObject({ code: DARK_POOL_ERROR_CODES.NOT_FOUND })
  })
})

describe('StoreMockClient.getOrderBook', () => {
  it('returns the live store snapshot', async () => {
    const { store, client } = freshClient()
    const resp = await client.getOrderBook({ pair: 'ETH/USDC' })
    expect(resp.bids).toEqual(store.getState().orderbook.bids)
    expect(resp.asks).toEqual(store.getState().orderbook.asks)
    expect(resp.pair).toBe('ETH/USDC')
  })

  it('falls back to the store pair when no pair is requested', async () => {
    const { client } = freshClient()
    const resp = await client.getOrderBook({ pair: '' })
    expect(resp.pair).toBe('ETH/USDC')
  })

  it('keeps the orderbook sorted (best bid first, best ask first)', async () => {
    const { client } = freshClient()
    const resp = await client.getOrderBook({ pair: 'ETH/USDC' })
    for (let i = 1; i < resp.bids.length; i++) {
      expect(new Decimal(resp.bids[i].price).lt(resp.bids[i - 1].price)).toBe(true)
    }
    for (let i = 1; i < resp.asks.length; i++) {
      expect(new Decimal(resp.asks[i].price).gt(resp.asks[i - 1].price)).toBe(true)
    }
  })
})

describe('StoreMockClient.getAuctionHistory', () => {
  it('returns the most recent auctions, capped by limit', async () => {
    const { store, client } = freshClient()
    store.getState().runAuction()
    const resp = await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 2 })
    )
    expect(resp.auctions).toHaveLength(2)
    expect(resp.auctions[0]).toBe(store.getState().recentAuctions[0])
  })

  it('returns all when limit is 0 (proto default)', async () => {
    const { store, client } = freshClient()
    const resp = await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 0 })
    )
    expect(resp.auctions).toHaveLength(store.getState().recentAuctions.length)
  })
})

describe('StoreMockClient.streamAuctions', () => {
  it('yields new auctions as they land in the store', async () => {
    const { store, client } = freshClient()
    const controller = new AbortController()
    const iter = client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }), {
      signal: controller.signal,
    })

    const events: string[] = []
    const drained = (async () => {
      for await (const ev of iter) {
        events.push(ev.auctionId)
        if (events.length >= 2) controller.abort()
      }
    })()

    // Yield once so the subscription registers, then drive two auctions.
    await Promise.resolve()
    const a = store.getState().runAuction()
    const b = store.getState().runAuction()
    await drained
    expect(events).toEqual([a.auctionId, b.auctionId])
  })

  it('terminates immediately on an already-aborted signal', async () => {
    const { client } = freshClient()
    const controller = new AbortController()
    controller.abort()
    const iter = client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }), {
      signal: controller.signal,
    })
    const events: unknown[] = []
    for await (const ev of iter) events.push(ev)
    expect(events).toEqual([])
  })

  it('filters by pair when one is set on the request', async () => {
    const { store, client } = freshClient()
    const controller = new AbortController()
    const iter = client.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'BTC/USDC' }), {
      signal: controller.signal,
    })
    const events: unknown[] = []
    const drained = (async () => {
      for await (const ev of iter) events.push(ev)
    })()

    await Promise.resolve()
    store.getState().runAuction() // emits an ETH/USDC auction
    // Give the subscribe callback a tick to run.
    await new Promise((r) => setTimeout(r, 5))
    controller.abort()
    await drained
    expect(events).toEqual([])
  })
})

// vi.useFakeTimers from the previous block must not leak into this file's
// async-tick assertions; vitest scopes timers per-test but the explicit
// teardown keeps that contract obvious to readers.
beforeEach(() => {
  vi.useRealTimers()
})
afterEach(() => {
  vi.useRealTimers()
})
