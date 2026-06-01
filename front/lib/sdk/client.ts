// DarkPoolClient — single seam between the trading UI and the engine.
//
// Phase 1 wires the UI against `MockClient` (in-memory, no network); Phase 2
// flips `NEXT_PUBLIC_USE_MOCKS=false` and the same UI hits `RestClient`.
// Per-method env flags (`NEXT_PUBLIC_USE_MOCKS_ORDERBOOK`, etc.) let Phase 2
// migrate one RPC at a time without flipping the whole app.
//
// Wire contract: the REST surface is grpc-gateway-compatible JSON over the
// service in crates/dp-api/proto/darkpool/v1/darkpool.proto. Bytes fields
// (`commitment`, `proof`, `encryptedPayload`) cross as standard base64
// strings; numeric strings (`price`, `size`, …) stay strings; int64s
// (`submittedAtUnix`, …) arrive as strings and `fromJson()` decodes them
// into `bigint` per the *Schema definitions.

import { create, fromJson, toJson } from '@bufbuild/protobuf'
import type { DescMessage, MessageShape } from '@bufbuild/protobuf'

import {
  AuctionEventSchema,
  CancelOrderResponseSchema,
  GetAuctionHistoryResponseSchema,
  GetOrderBookResponseSchema,
  GetOrderResponseSchema,
  OrderInfoSchema,
  PlaceOrderRequestSchema,
  PlaceOrderResponseSchema,
  Side,
} from './proto/darkpool/v1/darkpool_pb.js'
import type {
  AuctionEvent,
  CancelOrderRequest,
  CancelOrderResponse,
  GetAuctionHistoryRequest,
  GetAuctionHistoryResponse,
  GetOrderBookRequest,
  GetOrderBookResponse,
  GetOrderRequest,
  GetOrderResponse,
  PlaceOrderRequest,
  PlaceOrderResponse,
  StreamAuctionsRequest,
} from './proto/darkpool/v1/darkpool_pb.js'

// ─── Error model ──────────────────────────────────────────────────────────
//
// Mirrors `tonic::Code`. The REST layer returns
// `{ "code": <i32>, "message": "..." }` (see crates/dp-api/src/rest.rs::
// ApiError::into_response); we round-trip the numeric discriminant back so
// callers can `switch` on it without parsing strings. When the server only
// gives us an HTTP status (proxy errors, raw 5xx), we map back to a code
// using the inverse of the server's mapping.

export const DARK_POOL_ERROR_CODES = {
  OK: 0,
  CANCELLED: 1,
  UNKNOWN: 2,
  INVALID_ARGUMENT: 3,
  DEADLINE_EXCEEDED: 4,
  NOT_FOUND: 5,
  ALREADY_EXISTS: 6,
  PERMISSION_DENIED: 7,
  RESOURCE_EXHAUSTED: 8,
  FAILED_PRECONDITION: 9,
  ABORTED: 10,
  OUT_OF_RANGE: 11,
  UNIMPLEMENTED: 12,
  INTERNAL: 13,
  UNAVAILABLE: 14,
  DATA_LOSS: 15,
  UNAUTHENTICATED: 16,
} as const

export type DarkPoolErrorName = keyof typeof DARK_POOL_ERROR_CODES
export type DarkPoolErrorCode = (typeof DARK_POOL_ERROR_CODES)[DarkPoolErrorName]

const CODE_NAMES = new Map<DarkPoolErrorCode, DarkPoolErrorName>(
  (Object.entries(DARK_POOL_ERROR_CODES) as [DarkPoolErrorName, DarkPoolErrorCode][]).map(
    ([name, code]) => [code, name]
  )
)

export class DarkPoolError extends Error {
  readonly code: DarkPoolErrorCode
  readonly codeName: DarkPoolErrorName
  readonly httpStatus: number | null
  readonly retryable: boolean

  constructor(
    code: DarkPoolErrorCode,
    message: string,
    opts: { httpStatus?: number | null; cause?: unknown } = {}
  ) {
    super(message)
    this.name = 'DarkPoolError'
    this.code = code
    this.codeName = CODE_NAMES.get(code) ?? 'UNKNOWN'
    this.httpStatus = opts.httpStatus ?? null
    this.retryable = isRetryableCode(code)
    if (opts.cause !== undefined) {
      ;(this as { cause?: unknown }).cause = opts.cause
    }
  }
}

function isRetryableCode(code: DarkPoolErrorCode): boolean {
  return (
    code === DARK_POOL_ERROR_CODES.UNAVAILABLE ||
    code === DARK_POOL_ERROR_CODES.DEADLINE_EXCEEDED ||
    code === DARK_POOL_ERROR_CODES.ABORTED
  )
}

const KNOWN_CODE_VALUES = new Set<number>(Object.values(DARK_POOL_ERROR_CODES))

function codeFromBody(value: unknown): DarkPoolErrorCode | null {
  return typeof value === 'number' && KNOWN_CODE_VALUES.has(value)
    ? (value as DarkPoolErrorCode)
    : null
}

function codeFromHttpStatus(status: number): DarkPoolErrorCode {
  // Inverse of crates/dp-api/src/rest.rs::ApiError::into_response.
  switch (status) {
    case 400:
      return DARK_POOL_ERROR_CODES.INVALID_ARGUMENT
    case 401:
      return DARK_POOL_ERROR_CODES.UNAUTHENTICATED
    case 403:
      return DARK_POOL_ERROR_CODES.PERMISSION_DENIED
    case 404:
      return DARK_POOL_ERROR_CODES.NOT_FOUND
    case 409:
      return DARK_POOL_ERROR_CODES.ALREADY_EXISTS
    case 429:
      return DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED
    case 501:
      return DARK_POOL_ERROR_CODES.UNIMPLEMENTED
    case 503:
      return DARK_POOL_ERROR_CODES.UNAVAILABLE
    case 504:
      return DARK_POOL_ERROR_CODES.DEADLINE_EXCEEDED
    default:
      if (status >= 500) return DARK_POOL_ERROR_CODES.INTERNAL
      return DARK_POOL_ERROR_CODES.UNKNOWN
  }
}

// ─── Client interface ─────────────────────────────────────────────────────

export interface StreamOptions {
  signal?: AbortSignal
}

export interface DarkPoolClient {
  placeOrder(req: PlaceOrderRequest): Promise<PlaceOrderResponse>
  cancelOrder(req: CancelOrderRequest): Promise<CancelOrderResponse>
  getOrder(req: GetOrderRequest): Promise<GetOrderResponse>
  getOrderBook(req: GetOrderBookRequest): Promise<GetOrderBookResponse>
  getAuctionHistory(req: GetAuctionHistoryRequest): Promise<GetAuctionHistoryResponse>
  streamAuctions(req: StreamAuctionsRequest, opts?: StreamOptions): AsyncIterable<AuctionEvent>
}

export const DARK_POOL_METHODS = [
  'placeOrder',
  'cancelOrder',
  'getOrder',
  'getOrderBook',
  'getAuctionHistory',
  'streamAuctions',
] as const satisfies readonly (keyof DarkPoolClient)[]

export type DarkPoolMethod = (typeof DARK_POOL_METHODS)[number]

// ─── RestClient ───────────────────────────────────────────────────────────

export interface RestClientOptions {
  baseUrl: string
  apiKey: string
  /** Defaults to globalThis.fetch. Injectable so tests and SSR can swap it. */
  fetch?: typeof fetch
}

export class RestClient implements DarkPoolClient {
  private readonly baseUrl: string
  private readonly apiKey: string
  private readonly fetchImpl: typeof fetch

  constructor(opts: RestClientOptions) {
    this.baseUrl = opts.baseUrl.replace(/\/+$/, '')
    this.apiKey = opts.apiKey
    this.fetchImpl = opts.fetch ?? globalThis.fetch.bind(globalThis)
  }

  placeOrder(req: PlaceOrderRequest): Promise<PlaceOrderResponse> {
    // toJson() honours the proto bytes→base64 spec, so commitment/proof/
    // encryptedPayload arrive in the camelCase JSON shape the rest handler
    // expects (PlaceOrderJson in crates/dp-api/src/rest.rs).
    const body = toJson(PlaceOrderRequestSchema, req)
    return this.requestJson('POST', '/v1/orders', PlaceOrderResponseSchema, body)
  }

  cancelOrder(req: CancelOrderRequest): Promise<CancelOrderResponse> {
    const search = new URLSearchParams()
    if (req.reason) search.set('reason', req.reason)
    const qs = search.toString()
    return this.requestJson(
      'DELETE',
      `/v1/orders/${encodeURIComponent(req.orderId)}${qs ? `?${qs}` : ''}`,
      CancelOrderResponseSchema
    )
  }

  getOrder(req: GetOrderRequest): Promise<GetOrderResponse> {
    return this.requestJson(
      'GET',
      `/v1/orders/${encodeURIComponent(req.orderId)}`,
      GetOrderResponseSchema
    )
  }

  getOrderBook(req: GetOrderBookRequest): Promise<GetOrderBookResponse> {
    const search = new URLSearchParams()
    if (req.pair) search.set('pair', req.pair)
    const qs = search.toString()
    return this.requestJson('GET', `/v1/orderbook${qs ? `?${qs}` : ''}`, GetOrderBookResponseSchema)
  }

  getAuctionHistory(req: GetAuctionHistoryRequest): Promise<GetAuctionHistoryResponse> {
    const search = new URLSearchParams()
    if (req.pair) search.set('pair', req.pair)
    if (req.limit > 0) search.set('limit', String(req.limit))
    const qs = search.toString()
    return this.requestJson(
      'GET',
      `/v1/auctions${qs ? `?${qs}` : ''}`,
      GetAuctionHistoryResponseSchema
    )
  }

  async *streamAuctions(
    req: StreamAuctionsRequest,
    opts?: StreamOptions
  ): AsyncIterable<AuctionEvent> {
    const signal = opts?.signal
    if (signal?.aborted) return

    const search = new URLSearchParams()
    if (req.pair) search.set('pair', req.pair)
    const qs = search.toString()
    const url = `${this.baseUrl}/v1/auctions/stream${qs ? `?${qs}` : ''}`

    let response: Response
    try {
      response = await this.fetchImpl(url, {
        method: 'GET',
        headers: { accept: 'text/event-stream', 'x-api-key': this.apiKey },
        signal,
      })
    } catch (cause) {
      if (signal?.aborted) return
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.UNAVAILABLE,
        `Network error contacting ${url}: ${(cause as Error)?.message ?? cause}`,
        { cause }
      )
    }

    if (!response.ok) throw await parseErrorResponse(response)
    if (!response.body) {
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.UNAVAILABLE,
        `Auction stream from ${url} returned no body`
      )
    }

    const reader = response.body.getReader()
    const onAbort = () => {
      void reader.cancel()
    }
    signal?.addEventListener('abort', onAbort, { once: true })

    const decoder = new TextDecoder()
    let buffer = ''
    try {
      while (true) {
        let chunk
        try {
          chunk = await reader.read()
        } catch (cause) {
          if (signal?.aborted) return
          throw new DarkPoolError(
            DARK_POOL_ERROR_CODES.UNAVAILABLE,
            `Auction stream from ${url} dropped: ${(cause as Error)?.message ?? cause}`,
            { cause }
          )
        }
        if (chunk.done) return
        buffer += decoder.decode(chunk.value, { stream: true })

        let split = nextSseFrame(buffer)
        while (split !== null) {
          const frame = parseSseFrame(split.frame)
          buffer = split.rest
          if (frame !== null) {
            if (frame.event === 'error') {
              throw new DarkPoolError(
                DARK_POOL_ERROR_CODES.DATA_LOSS,
                `Auction stream lagged: ${readLagged(frame.data) ?? 'unknown'} events dropped`
              )
            }
            if ((frame.event === 'auction' || frame.event === '') && frame.data !== '') {
              const json = JSON.parse(frame.data) as unknown
              yield fromJson(
                AuctionEventSchema,
                json as Parameters<typeof fromJson<typeof AuctionEventSchema>>[1]
              )
            }
            // any other event type is ignored
          }
          split = nextSseFrame(buffer)
        }
      }
    } finally {
      signal?.removeEventListener('abort', onAbort)
      reader.releaseLock()
    }
  }

  private async requestJson<S extends DescMessage>(
    method: string,
    path: string,
    responseSchema: S,
    jsonBody?: unknown
  ): Promise<MessageShape<S>> {
    const url = `${this.baseUrl}${path}`
    const headers: Record<string, string> = {
      accept: 'application/json',
      'x-api-key': this.apiKey,
    }
    if (jsonBody !== undefined) headers['content-type'] = 'application/json'

    let response: Response
    try {
      response = await this.fetchImpl(url, {
        method,
        headers,
        body: jsonBody !== undefined ? JSON.stringify(jsonBody) : undefined,
      })
    } catch (cause) {
      throw new DarkPoolError(
        DARK_POOL_ERROR_CODES.UNAVAILABLE,
        `Network error contacting ${url}: ${(cause as Error)?.message ?? cause}`,
        { cause }
      )
    }

    if (!response.ok) throw await parseErrorResponse(response)

    const text = await response.text()
    const json = text === '' ? {} : (JSON.parse(text) as unknown)
    return fromJson(responseSchema, json as Parameters<typeof fromJson<S>>[1])
  }
}

// ─── SSE frame parsing (private to streamAuctions) ─────────────────────────

interface SseFrame {
  event: string
  data: string
}

// Find the next event boundary (a blank line). Handles both LF and CRLF
// servers; returns the frame text and the remaining buffer, or null if no
// complete frame has arrived yet.
function nextSseFrame(buffer: string): { frame: string; rest: string } | null {
  const lf = buffer.indexOf('\n\n')
  const crlf = buffer.indexOf('\r\n\r\n')
  let idx = -1
  let len = 0
  if (lf !== -1 && (crlf === -1 || lf < crlf)) {
    idx = lf
    len = 2
  } else if (crlf !== -1) {
    idx = crlf
    len = 4
  }
  if (idx === -1) return null
  return { frame: buffer.slice(0, idx), rest: buffer.slice(idx + len) }
}

// Parse one frame into its `event` (last wins, '' if absent) and `data`
// (multiple data: lines joined by \n, per the SSE spec). Comment (`:`) lines
// and other fields (id, retry) are ignored.
function parseSseFrame(frame: string): SseFrame | null {
  let event = ''
  const dataLines: string[] = []
  for (let line of frame.split('\n')) {
    if (line.endsWith('\r')) line = line.slice(0, -1)
    if (line === '' || line.startsWith(':')) continue
    const colon = line.indexOf(':')
    const field = colon === -1 ? line : line.slice(0, colon)
    let value = colon === -1 ? '' : line.slice(colon + 1)
    if (value.startsWith(' ')) value = value.slice(1)
    if (field === 'event') event = value
    else if (field === 'data') dataLines.push(value)
  }
  if (event === '' && dataLines.length === 0) return null
  return { event, data: dataLines.join('\n') }
}

function readLagged(data: string): number | null {
  try {
    const parsed = JSON.parse(data) as { lagged?: unknown }
    return typeof parsed.lagged === 'number' ? parsed.lagged : null
  } catch {
    return null
  }
}

async function parseErrorResponse(response: Response): Promise<DarkPoolError> {
  let body: { code?: unknown; message?: unknown } = {}
  try {
    const text = await response.text()
    if (text) body = JSON.parse(text) as typeof body
  } catch {
    // Non-JSON body — fall through to HTTP-status-based mapping.
  }
  const code = codeFromBody(body.code) ?? codeFromHttpStatus(response.status)
  const message =
    typeof body.message === 'string' && body.message.length > 0
      ? body.message
      : `HTTP ${response.status} from ${response.url}`
  return new DarkPoolError(code, message, { httpStatus: response.status })
}

// ─── MockClient ───────────────────────────────────────────────────────────
//
// Minimal in-memory client so the UI shell can mount without crashing while
// F1.2 (#69) is in flight. Storage is intentionally trivial: order placement
// gets a synthetic id, getOrder returns NOT_FOUND for unknown ids, and the
// orderbook/auction surfaces return empty payloads. #69 replaces this with
// the mock-store + faker factories.

export class MockClient implements DarkPoolClient {
  private readonly orders = new Map<string, ReturnType<typeof create<typeof OrderInfoSchema>>>()
  private nextId = 1
  private readonly pair: string

  constructor(opts: { pair?: string } = {}) {
    this.pair = opts.pair ?? 'ETH/USDC'
  }

  async placeOrder(req: PlaceOrderRequest): Promise<PlaceOrderResponse> {
    const id = `mock-${this.nextId++}`
    const order = create(OrderInfoSchema, {
      id,
      pair: this.pair,
      side: Side.BUY,
      price: '0',
      size: '0',
      remainingSize: '0',
      commitmentKey: bytesPreview(req.commitment),
      submittedAtUnix: BigInt(Math.floor(Date.now() / 1000)),
      expiresAtUnix: 0n,
    })
    this.orders.set(id, order)
    return create(PlaceOrderResponseSchema, { order })
  }

  async cancelOrder(req: CancelOrderRequest): Promise<CancelOrderResponse> {
    if (!this.orders.delete(req.orderId)) {
      throw new DarkPoolError(DARK_POOL_ERROR_CODES.NOT_FOUND, `Order ${req.orderId} not found`)
    }
    return create(CancelOrderResponseSchema, {})
  }

  async getOrder(req: GetOrderRequest): Promise<GetOrderResponse> {
    const order = this.orders.get(req.orderId)
    if (!order) {
      throw new DarkPoolError(DARK_POOL_ERROR_CODES.NOT_FOUND, `Order ${req.orderId} not found`)
    }
    return create(GetOrderResponseSchema, { order })
  }

  async getOrderBook(req: GetOrderBookRequest): Promise<GetOrderBookResponse> {
    return create(GetOrderBookResponseSchema, {
      pair: req.pair || this.pair,
      bids: [],
      asks: [],
    })
  }

  async getAuctionHistory(req: GetAuctionHistoryRequest): Promise<GetAuctionHistoryResponse> {
    void req
    return create(GetAuctionHistoryResponseSchema, { auctions: [] })
  }

  // eslint-disable-next-line require-yield
  async *streamAuctions(
    req: StreamAuctionsRequest,
    opts?: StreamOptions
  ): AsyncIterable<AuctionEvent> {
    void req
    // F1.2 (#69) replaces this with timer-driven synthetic events sourced
    // from the mock store. Until then: with a signal, block until aborted
    // so consumers can mirror the real stream's lifecycle; without one,
    // return immediately — otherwise the generator would await an
    // unresolved promise and iterator.return() couldn't free it (the
    // queued return completion only runs when the current await settles).
    const signal = opts?.signal
    if (!signal || signal.aborted) return
    await new Promise<void>((resolve) => {
      signal.addEventListener('abort', () => resolve(), { once: true })
    })
  }
}

function bytesPreview(b: Uint8Array): string {
  if (b.length === 0) return ''
  const slice = Array.from(b.slice(0, 4))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
  return `mock-commitment-${slice}`
}

// ─── Factory + per-method routing ─────────────────────────────────────────

export interface CreateDarkPoolClientOptions {
  baseUrl: string
  apiKey: string
  /** Global default. Overridden per-method by `methodOverrides`. */
  useMocks: boolean
  fetch?: typeof fetch
  /** Pre-built clients; useful for tests and SSR. */
  mockClient?: DarkPoolClient
  restClient?: DarkPoolClient
  /**
   * Per-method overrides. `true` forces the mock impl for that RPC,
   * `false` forces the rest impl, `undefined` falls back to `useMocks`.
   */
  methodOverrides?: Partial<Record<DarkPoolMethod, boolean>>
}

export function createDarkPoolClient(opts: CreateDarkPoolClientOptions): DarkPoolClient {
  const overrides = opts.methodOverrides ?? {}
  const hasOverrides = Object.values(overrides).some((v) => v !== undefined)

  const makeMock = (): DarkPoolClient => opts.mockClient ?? new MockClient()
  const makeRest = (): DarkPoolClient =>
    opts.restClient ??
    new RestClient({
      baseUrl: opts.baseUrl,
      apiKey: opts.apiKey,
      fetch: opts.fetch,
    })

  if (!hasOverrides) {
    return opts.useMocks ? makeMock() : makeRest()
  }

  let mockImpl: DarkPoolClient | null = null
  let restImpl: DarkPoolClient | null = null
  const pick = (m: DarkPoolMethod): DarkPoolClient => {
    const useMock = overrides[m] ?? opts.useMocks
    if (useMock) {
      mockImpl ??= makeMock()
      return mockImpl
    }
    restImpl ??= makeRest()
    return restImpl
  }
  return new RoutingClient(pick)
}

class RoutingClient implements DarkPoolClient {
  constructor(private readonly pick: (m: DarkPoolMethod) => DarkPoolClient) {}

  placeOrder(req: PlaceOrderRequest): Promise<PlaceOrderResponse> {
    return this.pick('placeOrder').placeOrder(req)
  }
  cancelOrder(req: CancelOrderRequest): Promise<CancelOrderResponse> {
    return this.pick('cancelOrder').cancelOrder(req)
  }
  getOrder(req: GetOrderRequest): Promise<GetOrderResponse> {
    return this.pick('getOrder').getOrder(req)
  }
  getOrderBook(req: GetOrderBookRequest): Promise<GetOrderBookResponse> {
    return this.pick('getOrderBook').getOrderBook(req)
  }
  getAuctionHistory(req: GetAuctionHistoryRequest): Promise<GetAuctionHistoryResponse> {
    return this.pick('getAuctionHistory').getAuctionHistory(req)
  }
  streamAuctions(req: StreamAuctionsRequest, opts?: StreamOptions): AsyncIterable<AuctionEvent> {
    return this.pick('streamAuctions').streamAuctions(req, opts)
  }
}

// ─── Env helpers ──────────────────────────────────────────────────────────

const METHOD_ENV_VAR: Record<DarkPoolMethod, string> = {
  placeOrder: 'NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER',
  cancelOrder: 'NEXT_PUBLIC_USE_MOCKS_CANCEL_ORDER',
  getOrder: 'NEXT_PUBLIC_USE_MOCKS_GET_ORDER',
  getOrderBook: 'NEXT_PUBLIC_USE_MOCKS_ORDERBOOK',
  getAuctionHistory: 'NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY',
  streamAuctions: 'NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS',
}

export function methodOverridesFromEnv(
  env: Record<string, string | undefined> = readPublicEnv()
): Partial<Record<DarkPoolMethod, boolean>> {
  const out: Partial<Record<DarkPoolMethod, boolean>> = {}
  for (const m of DARK_POOL_METHODS) {
    const raw = env[METHOD_ENV_VAR[m]]
    if (raw === undefined || raw === '') continue
    if (raw === 'true' || raw === '1') out[m] = true
    else if (raw === 'false' || raw === '0') out[m] = false
  }
  return out
}

// Inlined per-key reads keep Next's NEXT_PUBLIC_* inlining working at build
// time (NEXT_PUBLIC vars are statically replaced; bracket-access on
// process.env would defeat that).
function readPublicEnv(): Record<string, string | undefined> {
  return {
    NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER: process.env.NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER,
    NEXT_PUBLIC_USE_MOCKS_CANCEL_ORDER: process.env.NEXT_PUBLIC_USE_MOCKS_CANCEL_ORDER,
    NEXT_PUBLIC_USE_MOCKS_GET_ORDER: process.env.NEXT_PUBLIC_USE_MOCKS_GET_ORDER,
    NEXT_PUBLIC_USE_MOCKS_ORDERBOOK: process.env.NEXT_PUBLIC_USE_MOCKS_ORDERBOOK,
    NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY: process.env.NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY,
    NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS: process.env.NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS,
  }
}
