// Single source of truth for numeric conversion between
// the user-facing decimal world, the API wire format, and ERC-20 base
// units. See docs/adr/0002-decimals.md for the full contract.
//
// Never coerce a price/size through `Number(...)` or `parseFloat(...)`.
// Always go through these helpers.

import Decimal from 'decimal.js'
import { formatUnits, parseUnits } from 'viem'

export { Decimal }

export type DecimalInput = Decimal | string

export const TOKEN_DECIMALS = {
  WETH: 18,
  USDC: 6,
} as const

export type TokenSymbol = keyof typeof TOKEN_DECIMALS

export const WIRE_MAX_DP = 8
export const WIRE_MAX_SCALED = 2n ** 60n

const SCALE_FACTOR = new Decimal(10).pow(WIRE_MAX_DP)
const WIRE_MAX_RAW = new Decimal(WIRE_MAX_SCALED.toString()).div(SCALE_FACTOR)

// Force fixed-point string output across the full protocol range
// (~1e-8 up to ~1.15e10). The decimal.js defaults switch to scientific
// notation at 1e-7 and 1e21; widening both bounds keeps `toString()`
// canonical for every value units.ts will ever emit.
Decimal.set({ toExpNeg: -9, toExpPos: 30 })

export function toDecimal(value: DecimalInput): Decimal {
  if (value instanceof Decimal) return value
  if (typeof value !== 'string') {
    throw new TypeError(`units: expected Decimal or string, got ${typeof value}`)
  }
  const trimmed = value.trim()
  if (trimmed === '') {
    throw new RangeError('units: empty string is not a valid number')
  }
  let d: Decimal
  try {
    d = new Decimal(trimmed)
  } catch {
    throw new RangeError(`units: invalid decimal "${value}"`)
  }
  if (!d.isFinite()) {
    throw new RangeError(`units: non-finite decimal "${value}"`)
  }
  return d
}

function assertWireBounds(d: Decimal, field: 'size' | 'price'): void {
  if (d.isNegative()) {
    throw new RangeError(`units: ${field} must be non-negative (got ${d})`)
  }
  if (d.decimalPlaces() > WIRE_MAX_DP) {
    throw new RangeError(`units: ${field} exceeds ${WIRE_MAX_DP}dp wire precision (got ${d})`)
  }
  // Mirror crates/dp-zk/src/encoding.rs's 2^60 ceiling so we reject values
  // that would later fail proof generation.
  if (d.times(SCALE_FACTOR).gte(WIRE_MAX_SCALED.toString())) {
    throw new RangeError(
      `units: ${field} exceeds protocol max ${WIRE_MAX_RAW.toString()} (got ${d})`
    )
  }
}

function toWire(value: DecimalInput, field: 'size' | 'price'): string {
  const d = toDecimal(value)
  assertWireBounds(d, field)
  return d.toFixed() // canonical: no trailing zeros, no exponent
}

function fromWire(wire: string, field: 'size' | 'price'): Decimal {
  if (typeof wire !== 'string') {
    throw new TypeError(`units: wire ${field} must be a string`)
  }
  if (/[eE]/.test(wire)) {
    throw new RangeError(`units: wire ${field} must not use exponential notation (got "${wire}")`)
  }
  const d = toDecimal(wire)
  assertWireBounds(d, field)
  return d
}

export function toWireSize(value: DecimalInput): string {
  return toWire(value, 'size')
}

export function toWirePrice(value: DecimalInput): string {
  return toWire(value, 'price')
}

export function fromWireSize(wire: string): Decimal {
  return fromWire(wire, 'size')
}

export function fromWirePrice(wire: string): Decimal {
  return fromWire(wire, 'price')
}

function assertDecimals(decimals: number): void {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 30) {
    throw new RangeError(`units: invalid decimals ${decimals}`)
  }
}

export function toOnchainAmount(value: DecimalInput, decimals: number): bigint {
  assertDecimals(decimals)
  const d = toDecimal(value)
  if (d.isNegative()) {
    throw new RangeError(`units: on-chain amount must be non-negative (got ${d})`)
  }
  if (d.decimalPlaces() > decimals) {
    throw new RangeError(`units: amount ${d} exceeds ${decimals}dp for target token`)
  }
  return parseUnits(d.toFixed(), decimals)
}

export function fromOnchainAmount(value: bigint, decimals: number): Decimal {
  assertDecimals(decimals)
  if (typeof value !== 'bigint') {
    throw new TypeError(`units: expected bigint, got ${typeof value}`)
  }
  return new Decimal(formatUnits(value, decimals))
}

const DEFAULT_PRICE_DP = 2
const DEFAULT_SIZE_DP = 4

function formatFixed(value: DecimalInput, dp: number): string {
  if (!Number.isInteger(dp) || dp < 0) {
    throw new RangeError(`units: invalid display dp ${dp}`)
  }
  return toDecimal(value).toFixed(dp, Decimal.ROUND_HALF_UP)
}

export function formatPrice(value: DecimalInput, displayDp: number = DEFAULT_PRICE_DP): string {
  return formatFixed(value, displayDp)
}

export function formatSize(value: DecimalInput, displayDp: number = DEFAULT_SIZE_DP): string {
  return formatFixed(value, displayDp)
}
