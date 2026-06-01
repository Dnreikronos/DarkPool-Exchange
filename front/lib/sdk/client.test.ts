import { create } from '@bufbuild/protobuf'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  CancelOrderRequestSchema,
  GetAuctionHistoryRequestSchema,
  GetOrderBookRequestSchema,
  GetOrderRequestSchema,
  PlaceOrderRequestSchema,
  Side,
  StreamAuctionsRequestSchema,
} from './proto/darkpool/v1/darkpool_pb.js'
import {
  DARK_POOL_ERROR_CODES,
  DARK_POOL_METHODS,
  DarkPoolError,
  MockClient,
  RestClient,
  createDarkPoolClient,
  methodOverridesFromEnv,
  type DarkPoolClient,
  type DarkPoolMethod,
} from './client'

// ─── helpers ─────────────────────────────────────────────────────────────

type FetchCall = { url: string; init: RequestInit }

function makeJsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  })
}

function captureFetch(response: Response | (() => Response | Promise<Response>)) {
  const calls: FetchCall[] = []
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(input), init: init ?? {} })
    return typeof response === 'function' ? await response() : response
  }) as unknown as typeof globalThis.fetch
  return { fetch, calls }
}

const BASE = 'https://api.example.test'
const KEY = 'test-key'

// ─── RestClient: requests ─────────────────────────────────────────────────

describe('RestClient.placeOrder', () => {
  it('POSTs base64-encoded bytes to /v1/orders with the x-api-key header', async () => {
    const responseBody = {
      order: {
        id: 'order-1',
        pair: 'ETH/USDC',
        side: 'SIDE_BUY',
        price: '3000.50',
        size: '1.5',
        remainingSize: '1.5',
        commitmentKey: 'commit-abc',
        submittedAtUnix: '1700000000',
        expiresAtUnix: '0',
      },
    }
    const { fetch, calls } = captureFetch(makeJsonResponse(responseBody))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })

    const req = create(PlaceOrderRequestSchema, {
      commitment: new Uint8Array([0x01, 0x02, 0x03]),
      proof: new Uint8Array([0xff]),
      encryptedPayload: new Uint8Array([0xaa, 0xbb]),
    })
    const resp = await client.placeOrder(req)

    expect(calls).toHaveLength(1)
    const [call] = calls
    expect(call.url).toBe(`${BASE}/v1/orders`)
    expect(call.init.method).toBe('POST')
    const headers = call.init.headers as Record<string, string>
    expect(headers['x-api-key']).toBe(KEY)
    expect(headers['content-type']).toBe('application/json')
    expect(headers.accept).toBe('application/json')

    // commitment 010203 → "AQID", proof ff → "/w==", encryptedPayload aabb → "qrs="
    expect(JSON.parse(call.init.body as string)).toEqual({
      commitment: 'AQID',
      proof: '/w==',
      encryptedPayload: 'qrs=',
    })

    // Response decodes int64 timestamp to bigint per proto3 JSON mapping.
    expect(resp.order?.id).toBe('order-1')
    expect(resp.order?.side).toBe(Side.BUY)
    expect(resp.order?.submittedAtUnix).toBe(1700000000n)
  })

  it('strips trailing slashes from the base URL', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse({ order: undefined }))
    const client = new RestClient({ baseUrl: `${BASE}///`, apiKey: KEY, fetch })
    await client.placeOrder(create(PlaceOrderRequestSchema))
    expect(calls[0].url).toBe(`${BASE}/v1/orders`)
  })
})

describe('RestClient.cancelOrder', () => {
  it('issues DELETE with the reason as a query param and URL-encodes the id', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse({}))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })

    await client.cancelOrder(
      create(CancelOrderRequestSchema, {
        orderId: 'abc def',
        reason: 'user cancelled',
      })
    )

    expect(calls[0].init.method).toBe('DELETE')
    expect(calls[0].url).toBe(`${BASE}/v1/orders/abc%20def?reason=user+cancelled`)
  })

  it('omits the reason query param when empty', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse({}))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })

    await client.cancelOrder(create(CancelOrderRequestSchema, { orderId: 'abc', reason: '' }))

    expect(calls[0].url).toBe(`${BASE}/v1/orders/abc`)
  })
})

describe('RestClient.getOrder', () => {
  it('GETs /v1/orders/:id', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse({ order: undefined }))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await client.getOrder(create(GetOrderRequestSchema, { orderId: 'ord-9' }))
    expect(calls[0].init.method).toBe('GET')
    expect(calls[0].url).toBe(`${BASE}/v1/orders/ord-9`)
    expect(calls[0].init.body).toBeUndefined()
  })
})

describe('RestClient.getOrderBook', () => {
  it('GETs /v1/orderbook with the pair query param', async () => {
    const { fetch, calls } = captureFetch(
      makeJsonResponse({ pair: 'ETH/USDC', bids: [], asks: [] })
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const resp = await client.getOrderBook(create(GetOrderBookRequestSchema, { pair: 'ETH/USDC' }))
    expect(calls[0].url).toBe(`${BASE}/v1/orderbook?pair=ETH%2FUSDC`)
    expect(resp.bids).toEqual([])
  })
})

describe('RestClient.getAuctionHistory', () => {
  it('appends pair and limit when set, decodes int64 timestamp to bigint', async () => {
    const { fetch, calls } = captureFetch(
      makeJsonResponse({
        auctions: [
          {
            auctionId: 'a-1',
            pair: 'ETH/USDC',
            clearingPrice: '3000',
            matchedVolume: '5',
            matchCount: 3,
            timestampUnix: '1700000099',
          },
        ],
      })
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const resp = await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 50 })
    )
    expect(calls[0].url).toBe(`${BASE}/v1/auctions?pair=ETH%2FUSDC&limit=50`)
    expect(resp.auctions[0].timestampUnix).toBe(1700000099n)
  })

  it('omits limit when 0 (proto default = server-chosen)', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse({ auctions: [] }))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 0 })
    )
    expect(calls[0].url).toBe(`${BASE}/v1/auctions?pair=ETH%2FUSDC`)
  })
})

describe('RestClient.streamAuctions', () => {
  it('throws UNIMPLEMENTED — SSE bridge is not wired yet', async () => {
    const { fetch } = captureFetch(makeJsonResponse({}))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    const iter = client.streamAuctions({ $typeName: 'darkpool.v1.StreamAuctionsRequest', pair: '' })
    await expect(
      (async () => {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        for await (const _event of iter) {
          // unreachable
        }
      })()
    ).rejects.toMatchObject({
      name: 'DarkPoolError',
      code: DARK_POOL_ERROR_CODES.UNIMPLEMENTED,
    })
  })
})

// ─── RestClient: error mapping ────────────────────────────────────────────

describe('RestClient error mapping', () => {
  it('uses the body code when present', async () => {
    const { fetch } = captureFetch(
      new Response(
        JSON.stringify({
          code: DARK_POOL_ERROR_CODES.INVALID_ARGUMENT,
          message: 'bad price',
        }),
        { status: 400, headers: { 'content-type': 'application/json' } }
      )
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      client.getOrder(create(GetOrderRequestSchema, { orderId: 'x' }))
    ).rejects.toMatchObject({
      name: 'DarkPoolError',
      code: DARK_POOL_ERROR_CODES.INVALID_ARGUMENT,
      message: 'bad price',
      httpStatus: 400,
    })
  })

  it('falls back to HTTP-status mapping when the body has no code', async () => {
    const { fetch } = captureFetch(new Response('', { status: 401 }))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      client.getOrder(create(GetOrderRequestSchema, { orderId: 'x' }))
    ).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.UNAUTHENTICATED,
      httpStatus: 401,
    })
  })

  it('maps unknown body code to HTTP status mapping', async () => {
    const { fetch } = captureFetch(
      new Response(JSON.stringify({ code: 999, message: 'weird' }), {
        status: 503,
        headers: { 'content-type': 'application/json' },
      })
    )
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await expect(
      client.getOrderBook(create(GetOrderBookRequestSchema, { pair: 'ETH/USDC' }))
    ).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.UNAVAILABLE,
      retryable: true,
    })
  })

  it('treats fetch rejection as UNAVAILABLE with the original cause attached', async () => {
    const cause = new TypeError('connection refused')
    const fetch = vi.fn(async () => {
      throw cause
    }) as unknown as typeof globalThis.fetch
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    let thrown: unknown
    try {
      await client.getOrder(create(GetOrderRequestSchema, { orderId: 'x' }))
    } catch (e) {
      thrown = e
    }
    expect(thrown).toBeInstanceOf(DarkPoolError)
    expect((thrown as DarkPoolError).code).toBe(DARK_POOL_ERROR_CODES.UNAVAILABLE)
    expect((thrown as DarkPoolError & { cause?: unknown }).cause).toBe(cause)
  })
})

// ─── MockClient ───────────────────────────────────────────────────────────

describe('MockClient', () => {
  it('placeOrder stores the order so getOrder can read it back', async () => {
    const mock = new MockClient()
    const placed = await mock.placeOrder(
      create(PlaceOrderRequestSchema, {
        commitment: new Uint8Array([1, 2, 3, 4]),
      })
    )
    expect(placed.order?.id).toBe('mock-1')
    const fetched = await mock.getOrder(
      create(GetOrderRequestSchema, { orderId: placed.order!.id })
    )
    expect(fetched.order?.id).toBe('mock-1')
  })

  it('getOrder throws NOT_FOUND for unknown ids', async () => {
    const mock = new MockClient()
    await expect(
      mock.getOrder(create(GetOrderRequestSchema, { orderId: 'nope' }))
    ).rejects.toMatchObject({
      name: 'DarkPoolError',
      code: DARK_POOL_ERROR_CODES.NOT_FOUND,
    })
  })

  it('cancelOrder removes the order', async () => {
    const mock = new MockClient()
    const placed = await mock.placeOrder(create(PlaceOrderRequestSchema))
    await mock.cancelOrder(
      create(CancelOrderRequestSchema, {
        orderId: placed.order!.id,
        reason: 'user',
      })
    )
    await expect(
      mock.getOrder(create(GetOrderRequestSchema, { orderId: placed.order!.id }))
    ).rejects.toMatchObject({ code: DARK_POOL_ERROR_CODES.NOT_FOUND })
  })

  it('getOrderBook returns an empty book for the configured pair', async () => {
    const mock = new MockClient()
    const book = await mock.getOrderBook(create(GetOrderBookRequestSchema, { pair: '' }))
    expect(book.pair).toBe('ETH/USDC')
    expect(book.bids).toEqual([])
    expect(book.asks).toEqual([])
  })

  it('getAuctionHistory returns an empty list', async () => {
    const mock = new MockClient()
    const hist = await mock.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 10 })
    )
    expect(hist.auctions).toEqual([])
  })

  it('streamAuctions returns immediately when no signal is provided (no leaked generator)', async () => {
    const mock = new MockClient()
    const iter = mock.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }))
    // If the generator awaited an unresolved promise this would hang the
    // test runner; the 50 ms race guards against regressions.
    const events: unknown[] = []
    const drained = (async () => {
      for await (const ev of iter) events.push(ev)
      return 'done' as const
    })()
    const result = await Promise.race([
      drained,
      new Promise<'timeout'>((resolve) => setTimeout(() => resolve('timeout'), 50)),
    ])
    expect(result).toBe('done')
    expect(events).toEqual([])
  })

  it('streamAuctions blocks until the AbortSignal fires, then terminates cleanly', async () => {
    const mock = new MockClient()
    const controller = new AbortController()
    const iter = mock.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }), {
      signal: controller.signal,
    })
    let resolved = false
    const drained = (async () => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      for await (const _ev of iter) {
        // unreachable
      }
      resolved = true
    })()

    // Yield twice so the generator can settle on the await.
    await Promise.resolve()
    await Promise.resolve()
    expect(resolved).toBe(false)

    controller.abort()
    await drained
    expect(resolved).toBe(true)
  })

  it('streamAuctions terminates immediately when given an already-aborted signal', async () => {
    const mock = new MockClient()
    const controller = new AbortController()
    controller.abort()
    const iter = mock.streamAuctions(create(StreamAuctionsRequestSchema, { pair: 'ETH/USDC' }), {
      signal: controller.signal,
    })
    const events: unknown[] = []
    for await (const ev of iter) events.push(ev)
    expect(events).toEqual([])
  })
})

// ─── Factory ──────────────────────────────────────────────────────────────

describe('createDarkPoolClient', () => {
  it('returns MockClient when useMocks=true and no overrides', () => {
    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: true,
    })
    expect(client).toBeInstanceOf(MockClient)
  })

  it('returns RestClient when useMocks=false and no overrides', () => {
    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: false,
    })
    expect(client).toBeInstanceOf(RestClient)
  })

  it('routes per method when overrides are set, using the same impl across calls', async () => {
    const restCalls: DarkPoolMethod[] = []
    const mockCalls: DarkPoolMethod[] = []
    const stubRest = makeRecordingClient((m) => restCalls.push(m))
    const stubMock = makeRecordingClient((m) => mockCalls.push(m))

    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: true,
      mockClient: stubMock,
      restClient: stubRest,
      methodOverrides: {
        getOrderBook: false, // force REST
        placeOrder: false, // force REST
        // streamAuctions, getOrder, cancelOrder, getAuctionHistory fall back to global mock
      },
    })

    await client.getOrderBook(create(GetOrderBookRequestSchema, { pair: 'ETH/USDC' }))
    await client.placeOrder(create(PlaceOrderRequestSchema))
    await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 0 })
    )

    expect(restCalls).toEqual(['getOrderBook', 'placeOrder'])
    expect(mockCalls).toEqual(['getAuctionHistory'])
  })

  it('exposes every interface method via DARK_POOL_METHODS', () => {
    const stubMock = makeRecordingClient(() => undefined)
    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: true,
      mockClient: stubMock,
    })
    for (const m of DARK_POOL_METHODS) {
      expect(typeof (client as unknown as Record<string, unknown>)[m]).toBe('function')
    }
  })
})

function makeRecordingClient(onCall: (m: DarkPoolMethod) => void): DarkPoolClient {
  return {
    async placeOrder() {
      onCall('placeOrder')
      return { $typeName: 'darkpool.v1.PlaceOrderResponse' } as never
    },
    async cancelOrder() {
      onCall('cancelOrder')
      return { $typeName: 'darkpool.v1.CancelOrderResponse' } as never
    },
    async getOrder() {
      onCall('getOrder')
      return { $typeName: 'darkpool.v1.GetOrderResponse' } as never
    },
    async getOrderBook() {
      onCall('getOrderBook')
      return { $typeName: 'darkpool.v1.GetOrderBookResponse' } as never
    },
    async getAuctionHistory() {
      onCall('getAuctionHistory')
      return { $typeName: 'darkpool.v1.GetAuctionHistoryResponse' } as never
    },
    streamAuctions() {
      onCall('streamAuctions')
      return (async function* () {})()
    },
  }
}

// ─── methodOverridesFromEnv ───────────────────────────────────────────────

describe('methodOverridesFromEnv', () => {
  it('parses true/false/1/0 and skips empty/undefined', () => {
    expect(
      methodOverridesFromEnv({
        NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER: 'true',
        NEXT_PUBLIC_USE_MOCKS_CANCEL_ORDER: '0',
        NEXT_PUBLIC_USE_MOCKS_GET_ORDER: 'false',
        NEXT_PUBLIC_USE_MOCKS_ORDERBOOK: '1',
        NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY: '',
        NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS: undefined,
      })
    ).toEqual({
      placeOrder: true,
      cancelOrder: false,
      getOrder: false,
      getOrderBook: true,
    })
  })

  it('returns an empty object when nothing is set', () => {
    expect(methodOverridesFromEnv({})).toEqual({})
  })
})

// ─── I2.4 wire test: NEXT_PUBLIC_USE_MOCKS_ORDERBOOK=false flips just  ──
//      `getOrderBook` to the live RestClient while everything else stays
//      on the StoreMockClient. Locks the documented Phase-2 ramp so a
//      regression in either the env parser or the routing client surfaces
//      here instead of as a silent mocked response in the browser.

describe('Phase 2 flip — NEXT_PUBLIC_USE_MOCKS_ORDERBOOK=false', () => {
  it('routes getOrderBook to REST while siblings stay on the StoreMockClient', async () => {
    const overrides = methodOverridesFromEnv({
      NEXT_PUBLIC_USE_MOCKS_ORDERBOOK: 'false',
    })
    expect(overrides).toEqual({ getOrderBook: false })

    const { fetch, calls } = captureFetch(
      makeJsonResponse({ pair: 'ETH/USDC', bids: [], asks: [] })
    )
    const mockCalls: DarkPoolMethod[] = []
    const stubMock = makeRecordingClient((m) => mockCalls.push(m))

    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: true,
      mockClient: stubMock,
      restClient: new RestClient({ baseUrl: BASE, apiKey: KEY, fetch }),
      methodOverrides: overrides,
    })

    await client.getOrderBook(create(GetOrderBookRequestSchema, { pair: 'ETH/USDC' }))
    await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 50 })
    )

    expect(calls).toHaveLength(1)
    expect(calls[0].url).toBe(`${BASE}/v1/orderbook?pair=ETH%2FUSDC`)
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers['x-api-key']).toBe(KEY)
    expect(mockCalls).toEqual(['getAuctionHistory'])
  })
})

// ─── I2.5 wire test: NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY=false flips ──
//      just `getAuctionHistory` to the live RestClient (siblings stay on
//      the StoreMockClient). Mirrors the I2.4 orderbook flip so a
//      regression in either the env parser or the routing client surfaces
//      here instead of as a silent mocked tape in the browser.

describe('Phase 2 flip — NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY=false', () => {
  it('routes getAuctionHistory to REST while siblings stay on the StoreMockClient', async () => {
    const overrides = methodOverridesFromEnv({
      NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY: 'false',
    })
    expect(overrides).toEqual({ getAuctionHistory: false })

    const { fetch, calls } = captureFetch(makeJsonResponse({ auctions: [] }))
    const mockCalls: DarkPoolMethod[] = []
    const stubMock = makeRecordingClient((m) => mockCalls.push(m))

    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: true,
      mockClient: stubMock,
      restClient: new RestClient({ baseUrl: BASE, apiKey: KEY, fetch }),
      methodOverrides: overrides,
    })

    await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 50 })
    )
    await client.getOrderBook(create(GetOrderBookRequestSchema, { pair: 'ETH/USDC' }))

    expect(calls).toHaveLength(1)
    expect(calls[0].url).toBe(`${BASE}/v1/auctions?pair=ETH%2FUSDC&limit=50`)
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers['x-api-key']).toBe(KEY)
    expect(mockCalls).toEqual(['getOrderBook'])
  })

  it('falls through to the empty-array response when the backend reports zero auctions', async () => {
    const { fetch } = captureFetch(makeJsonResponse({ auctions: [] }))
    const client = createDarkPoolClient({
      baseUrl: BASE,
      apiKey: KEY,
      useMocks: false,
      restClient: new RestClient({ baseUrl: BASE, apiKey: KEY, fetch }),
    })
    const resp = await client.getAuctionHistory(
      create(GetAuctionHistoryRequestSchema, { pair: 'ETH/USDC', limit: 50 })
    )
    expect(resp.auctions).toEqual([])
  })
})

// ─── DarkPoolError ────────────────────────────────────────────────────────

describe('DarkPoolError', () => {
  it('exposes the named code and marks transient codes as retryable', () => {
    const e = new DarkPoolError(DARK_POOL_ERROR_CODES.UNAVAILABLE, 'engine restarting')
    expect(e.codeName).toBe('UNAVAILABLE')
    expect(e.retryable).toBe(true)
  })

  it('marks deterministic codes as non-retryable', () => {
    const e = new DarkPoolError(DARK_POOL_ERROR_CODES.INVALID_ARGUMENT, 'bad input')
    expect(e.retryable).toBe(false)
  })
})

beforeEach(() => {
  // No-op; vi.restoreAllMocks() would clobber the spies created in each test.
})

// ─── RestClient: SIWE bearer token injection ───────────────────────────────

describe('RestClient auth header', () => {
  const orderBody = {
    order: {
      id: 'o1',
      pair: 'ETH/USDC',
      side: 'SIDE_BUY',
      price: '1',
      size: '1',
      remainingSize: '1',
      commitmentKey: 'c',
      submittedAtUnix: '1700000000',
      expiresAtUnix: '0',
    },
  }
  const aRequest = () => create(PlaceOrderRequestSchema, { commitment: new Uint8Array([0x01]) })

  it('sends Authorization: Bearer alongside x-api-key when getToken returns a token', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse(orderBody))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch, getToken: () => 'jwt-123' })
    await client.placeOrder(aRequest())
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers['authorization']).toBe('Bearer jwt-123')
    expect(headers['x-api-key']).toBe(KEY)
  })

  it('omits Authorization when getToken returns null (x-api-key only)', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse(orderBody))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch, getToken: () => null })
    await client.placeOrder(aRequest())
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers['authorization']).toBeUndefined()
    expect(headers['x-api-key']).toBe(KEY)
  })

  it('omits Authorization entirely when no getToken is configured', async () => {
    const { fetch, calls } = captureFetch(makeJsonResponse(orderBody))
    const client = new RestClient({ baseUrl: BASE, apiKey: KEY, fetch })
    await client.placeOrder(aRequest())
    const headers = calls[0].init.headers as Record<string, string>
    expect(headers['authorization']).toBeUndefined()
  })

  it('calls onUnauthenticated once on a 401 before throwing UNAUTHENTICATED', async () => {
    const onUnauthenticated = vi.fn()
    const { fetch } = captureFetch(makeJsonResponse({ message: 'expired' }, { status: 401 }))
    const client = new RestClient({
      baseUrl: BASE,
      apiKey: KEY,
      fetch,
      getToken: () => 'jwt',
      onUnauthenticated,
    })
    await expect(client.placeOrder(aRequest())).rejects.toMatchObject({
      code: DARK_POOL_ERROR_CODES.UNAUTHENTICATED,
    })
    expect(onUnauthenticated).toHaveBeenCalledTimes(1)
  })

  it('does not call onUnauthenticated on a non-401 error', async () => {
    const onUnauthenticated = vi.fn()
    const { fetch } = captureFetch(makeJsonResponse({ message: 'boom' }, { status: 500 }))
    const client = new RestClient({
      baseUrl: BASE,
      apiKey: KEY,
      fetch,
      getToken: () => 'jwt',
      onUnauthenticated,
    })
    await expect(client.placeOrder(aRequest())).rejects.toBeInstanceOf(DarkPoolError)
    expect(onUnauthenticated).not.toHaveBeenCalled()
  })
})
