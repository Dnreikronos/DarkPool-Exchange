// Typed faker factories for the simulated backend that powers Phase 1 of
// the trading app. Every output is shaped against the proto types generated
// by F0.3 (../proto/darkpool/v1/darkpool_pb.ts) — no parallel type
// definitions live here. Decimal math always goes through Decimal/units.ts
// so wire fields stay canonical decimal strings (see docs/adr/0002-decimals.md).
//
// `createFactoryContext({ seed })` returns a deterministic Faker handle:
// tests pass a fixed seed for reproducible fixtures, runtime callers omit
// the seed for fresh randomness on every tick.

import { create } from '@bufbuild/protobuf'
import { Faker, en } from '@faker-js/faker'

import { Decimal, toWirePrice, toWireSize } from '../../units'

import { AuctionSummarySchema, OrderInfoSchema, Side } from '../proto/darkpool/v1/darkpool_pb.js'
import type { AuctionSummary, OrderInfo } from '../proto/darkpool/v1/darkpool_pb.js'
// Order book is mock-only since #178 — hand-written types, not generated.
import type { OrderBook, PriceLevel } from '../orderbook.js'

// ─── Pair / market defaults ───────────────────────────────────────────────
//
// MVP is single-pair ETH/USDC (see CLAUDE.md "Locked-in product decisions").
// Multi-pair gates on backend #29 (pair registry); when that lands we keep
// the same shape but read the default from a config.

export const DEFAULT_PAIR = 'ETH/USDC'
export const DEFAULT_MID = new Decimal('3000')

/**
 * Display precision the orderbook snaps prices to. Levels are spaced by
 * 10^(-PRICE_DP) USDC; the perturbation tick rounds to this grid so the
 * book doesn't visibly drift off-tick between renders.
 */
export const PRICE_DP = 2
/**
 * Display precision sizes snap to. Wire allows 8dp; the panels only show 4.
 */
export const SIZE_DP = 4

// ─── Local Fill type ──────────────────────────────────────────────────────
//
// The proto has no Fill message — a Fill is an entry in the mock fill
// history, surfaced by F1.11 (#78). When/if the backend gains a Fill
// message we replace this with the generated type.

export interface Fill {
  fillId: string
  orderId: string
  auctionId: string
  side: Side
  price: string
  size: string
  timestampUnix: bigint
}

export interface Balances {
  weth: string
  usdc: string
}

// ─── Factory context ──────────────────────────────────────────────────────

export interface FactoryContextOptions {
  /** Seed the underlying Faker for deterministic test output. */
  seed?: number
  /** Market pair the factories tag onto generated messages. */
  pair?: string
  /** Injectable clock so tests can pin timestamp_unix. */
  now?: () => number
}

export interface FactoryContext {
  readonly faker: Faker
  readonly pair: string
  /** Returns the current unix timestamp in seconds. */
  readonly now: () => number
}

export function createFactoryContext(opts: FactoryContextOptions = {}): FactoryContext {
  const faker = new Faker({ locale: en })
  if (opts.seed !== undefined) faker.seed(opts.seed)
  return {
    faker,
    pair: opts.pair ?? DEFAULT_PAIR,
    now: opts.now ?? (() => Math.floor(Date.now() / 1000)),
  }
}

// ─── Decimal sampling ─────────────────────────────────────────────────────
//
// faker.number.float returns a JS number; routing that straight into a
// Decimal works for small magnitudes but invites binary-float drift on
// values like 0.1. We sample an integer in scaled-up space and divide by
// 10^dp inside Decimal so the result is exact at the target precision.

function sampleDecimal(
  faker: Faker,
  range: { min: Decimal | string; max: Decimal | string; dp: number }
): Decimal {
  const dp = range.dp
  if (!Number.isInteger(dp) || dp < 0 || dp > 8) {
    throw new RangeError(`factories: dp out of range [0, 8] (got ${dp})`)
  }
  const min = range.min instanceof Decimal ? range.min : new Decimal(range.min)
  const max = range.max instanceof Decimal ? range.max : new Decimal(range.max)
  if (max.lt(min)) {
    throw new RangeError(`factories: max < min (${max} < ${min})`)
  }
  const scale = new Decimal(10).pow(dp)
  const minScaled = min.times(scale).round().toNumber()
  const maxScaled = max.times(scale).round().toNumber()
  const sampled = faker.number.int({ min: minScaled, max: maxScaled })
  return new Decimal(sampled).div(scale)
}

// ─── mockPriceLevel ───────────────────────────────────────────────────────

export interface MockPriceLevelOptions {
  price: Decimal | string
  totalSize?: Decimal | string
  orderCount?: number
}

export function mockPriceLevel(ctx: FactoryContext, opts: MockPriceLevelOptions): PriceLevel {
  const size =
    opts.totalSize !== undefined
      ? opts.totalSize
      : sampleDecimal(ctx.faker, { min: '0.5', max: '50', dp: SIZE_DP })
  const orderCount = opts.orderCount ?? ctx.faker.number.int({ min: 1, max: 12 })
  return {
    price: toWirePrice(opts.price),
    totalSize: toWireSize(size),
    orderCount,
  }
}

// ─── mockOrderBook ────────────────────────────────────────────────────────

export interface MockOrderBookOptions {
  /** Mid price the bids/asks straddle. */
  mid?: Decimal | string
  /** How many levels per side. Defaults to 12. */
  depth?: number
  /** Distance (in USDC) between adjacent levels. Defaults to 1. */
  tickSize?: Decimal | string
  /** Optional override for the pair name. */
  pair?: string
}

export function mockOrderBook(ctx: FactoryContext, opts: MockOrderBookOptions = {}): OrderBook {
  const mid = opts.mid !== undefined ? new Decimal(opts.mid.toString()) : DEFAULT_MID
  const depth = opts.depth ?? 12
  const tick = opts.tickSize !== undefined ? new Decimal(opts.tickSize.toString()) : new Decimal(1)
  const pair = opts.pair ?? ctx.pair

  const bids: PriceLevel[] = []
  const asks: PriceLevel[] = []
  for (let i = 1; i <= depth; i++) {
    const bidPrice = mid.minus(tick.times(i))
    const askPrice = mid.plus(tick.times(i))
    if (bidPrice.gt(0)) bids.push(mockPriceLevel(ctx, { price: bidPrice }))
    asks.push(mockPriceLevel(ctx, { price: askPrice }))
  }
  // Proto repeated fields don't enforce ordering; we sort for the UI so the
  // top of book is always the first element (panels read [0] without a
  // comparator).
  bids.sort((a, b) => new Decimal(b.price).cmp(new Decimal(a.price)))
  asks.sort((a, b) => new Decimal(a.price).cmp(new Decimal(b.price)))

  return { pair, bids, asks }
}

// ─── mockOrderInfo ────────────────────────────────────────────────────────

export interface MockOrderInfoOptions {
  id?: string
  side?: Side
  price?: Decimal | string
  size?: Decimal | string
  remainingSize?: Decimal | string
  commitmentKey?: string
  submittedAtUnix?: bigint
  expiresAtUnix?: bigint
  pair?: string
}

export function mockOrderInfo(ctx: FactoryContext, opts: MockOrderInfoOptions = {}): OrderInfo {
  const side =
    opts.side ?? (ctx.faker.datatype.boolean({ probability: 0.5 }) ? Side.BUY : Side.SELL)
  const price =
    opts.price !== undefined
      ? new Decimal(opts.price.toString())
      : DEFAULT_MID.plus(sampleDecimal(ctx.faker, { min: '-20', max: '20', dp: PRICE_DP }))
  const size =
    opts.size !== undefined
      ? new Decimal(opts.size.toString())
      : sampleDecimal(ctx.faker, { min: '0.1', max: '5', dp: SIZE_DP })
  const remainingSize =
    opts.remainingSize !== undefined ? new Decimal(opts.remainingSize.toString()) : size

  return create(OrderInfoSchema, {
    id: opts.id ?? `mock-${ctx.faker.string.uuid()}`,
    pair: opts.pair ?? ctx.pair,
    side,
    price: toWirePrice(price),
    size: toWireSize(size),
    remainingSize: toWireSize(remainingSize),
    commitmentKey:
      opts.commitmentKey ??
      `mock-${ctx.faker.string.hexadecimal({ length: 8, casing: 'lower', prefix: '' })}`,
    submittedAtUnix: opts.submittedAtUnix ?? BigInt(ctx.now()),
    expiresAtUnix: opts.expiresAtUnix ?? 0n,
  })
}

// ─── mockAuctionSummary ───────────────────────────────────────────────────

export interface MockAuctionSummaryOptions {
  auctionId?: string
  pair?: string
  /** Mid the clearing price is drawn from; defaults to DEFAULT_MID. */
  mid?: Decimal | string
  /** ± noise around mid in absolute price units. Defaults to 0.5% of mid. */
  noise?: Decimal | string
  clearingPrice?: Decimal | string
  matchedVolume?: Decimal | string
  matchCount?: number
  timestampUnix?: bigint
}

export function mockAuctionSummary(
  ctx: FactoryContext,
  opts: MockAuctionSummaryOptions = {}
): AuctionSummary {
  const mid = opts.mid !== undefined ? new Decimal(opts.mid.toString()) : DEFAULT_MID
  const noise = opts.noise !== undefined ? new Decimal(opts.noise.toString()) : mid.times('0.005')
  const clearingPrice =
    opts.clearingPrice !== undefined
      ? new Decimal(opts.clearingPrice.toString())
      : mid.plus(sampleDecimal(ctx.faker, { min: noise.neg(), max: noise, dp: PRICE_DP }))

  const matchedVolume =
    opts.matchedVolume !== undefined
      ? new Decimal(opts.matchedVolume.toString())
      : sampleDecimal(ctx.faker, { min: '0.01', max: '12', dp: SIZE_DP })

  return create(AuctionSummarySchema, {
    auctionId:
      opts.auctionId ?? `auc-${ctx.faker.string.alphanumeric({ length: 10, casing: 'lower' })}`,
    pair: opts.pair ?? ctx.pair,
    clearingPrice: toWirePrice(clearingPrice),
    matchedVolume: toWireSize(matchedVolume),
    matchCount: opts.matchCount ?? ctx.faker.number.int({ min: 0, max: 24 }),
    timestampUnix: opts.timestampUnix ?? BigInt(ctx.now()),
  })
}

// ─── mockFill ─────────────────────────────────────────────────────────────

export interface MockFillOptions {
  fillId?: string
  orderId: string
  auctionId?: string
  side: Side
  price: Decimal | string
  size: Decimal | string
  timestampUnix?: bigint
}

export function mockFill(ctx: FactoryContext, opts: MockFillOptions): Fill {
  return {
    fillId: opts.fillId ?? `fill-${ctx.faker.string.alphanumeric({ length: 8, casing: 'lower' })}`,
    orderId: opts.orderId,
    auctionId:
      opts.auctionId ?? `auc-${ctx.faker.string.alphanumeric({ length: 10, casing: 'lower' })}`,
    side: opts.side,
    price: toWirePrice(opts.price),
    size: toWireSize(opts.size),
    timestampUnix: opts.timestampUnix ?? BigInt(ctx.now()),
  }
}

// ─── mockBalances ─────────────────────────────────────────────────────────

export interface MockBalancesOptions {
  weth?: Decimal | string
  usdc?: Decimal | string
}

export function mockBalances(_ctx: FactoryContext, opts: MockBalancesOptions = {}): Balances {
  const weth = opts.weth !== undefined ? new Decimal(opts.weth.toString()) : new Decimal('10')
  const usdc = opts.usdc !== undefined ? new Decimal(opts.usdc.toString()) : new Decimal('25000')
  return {
    weth: toWireSize(weth),
    usdc: toWireSize(usdc),
  }
}

// ─── Helpers for the store ────────────────────────────────────────────────
//
// The mock-store layers small per-tick mutations on top of these factories.
// Exposing the primitives keeps the store from re-implementing decimal math
// inline.

/**
 * Multiply a wire-string size by a Decimal scalar and return a new wire
 * string. Used by the 1s perturb tick. The product is truncated to
 * SIZE_DP so repeated perturbations don't drift past the wire's 8dp
 * ceiling, and floored at `min` so a level never disappears through
 * repeated downward perturbations.
 */
export function scaleWireSize(
  wire: string,
  scalar: Decimal | string,
  min: Decimal | string = '0.0001'
): string {
  const product = new Decimal(wire).times(scalar.toString())
  const snapped = product.toDecimalPlaces(SIZE_DP, Decimal.ROUND_DOWN)
  const floor = new Decimal(min.toString())
  return toWireSize(snapped.lt(floor) ? floor : snapped)
}

/** Return the midpoint of best bid / best ask. Throws if either side is empty. */
export function midFromBook(book: OrderBook): Decimal {
  if (book.bids.length === 0 || book.asks.length === 0) {
    throw new Error('factories: cannot compute mid on an empty side')
  }
  const bestBid = new Decimal(book.bids[0].price)
  const bestAsk = new Decimal(book.asks[0].price)
  return bestBid.plus(bestAsk).div(2)
}
