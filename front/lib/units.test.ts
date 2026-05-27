import { describe, expect, it } from 'vitest'

import {
  Decimal,
  TOKEN_DECIMALS,
  WIRE_MAX_DP,
  WIRE_MAX_SCALED,
  formatPrice,
  formatSize,
  fromOnchainAmount,
  fromWirePrice,
  fromWireSize,
  toDecimal,
  toOnchainAmount,
  toWirePrice,
  toWireSize,
} from './units'

describe('toDecimal', () => {
  it('accepts a Decimal and returns it unchanged', () => {
    const d = new Decimal('3000.5')
    expect(toDecimal(d)).toBe(d)
  })

  it('parses a canonical decimal string', () => {
    expect(toDecimal('3000.5').toString()).toBe('3000.5')
  })

  it('parses exponential strings into the underlying value', () => {
    expect(toDecimal('1e3').toString()).toBe('1000')
  })

  it('rejects empty strings', () => {
    expect(() => toDecimal('')).toThrow(/empty/)
  })

  it('rejects non-numeric strings', () => {
    expect(() => toDecimal('not a number')).toThrow(/invalid/)
  })

  it('rejects non-string, non-Decimal input', () => {
    // @ts-expect-error: intentional misuse to ensure runtime guard fires
    expect(() => toDecimal(42)).toThrow(TypeError)
    // @ts-expect-error: intentional misuse to ensure runtime guard fires
    expect(() => toDecimal(null)).toThrow(TypeError)
  })
})

describe('wire format', () => {
  it('toWireSize emits canonical fixed-point strings', () => {
    expect(toWireSize('3000.50')).toBe('3000.5') // strips trailing zero
    expect(toWireSize('3000')).toBe('3000')
    expect(toWireSize('0.00000001')).toBe('0.00000001') // 8dp, smallest valid
    expect(toWireSize('0')).toBe('0')
  })

  it('toWirePrice emits canonical fixed-point strings', () => {
    expect(toWirePrice('0.12345678')).toBe('0.12345678')
    expect(toWirePrice(new Decimal('1500'))).toBe('1500')
  })

  it('rejects negatives', () => {
    expect(() => toWireSize('-1')).toThrow(/non-negative/)
    expect(() => toWirePrice('-0.5')).toThrow(/non-negative/)
  })

  it('rejects > 8dp precision', () => {
    expect(() => toWireSize('0.000000001')).toThrow(/8dp/) // 9dp
    expect(() => toWirePrice('3000.123456789')).toThrow(/8dp/)
  })

  it('accepts exactly 8dp', () => {
    expect(toWireSize('0.12345678')).toBe('0.12345678')
  })

  it('rejects values >= 2^60 / 1e8 (~1.15e10)', () => {
    // 2^60 / 1e8 ≈ 11_529_215_046.0686976
    const overflow = new Decimal(WIRE_MAX_SCALED.toString()).div('1e8')
    expect(() => toWireSize(overflow)).toThrow(/protocol max/)
    expect(() => toWireSize(overflow.plus('0.00000001'))).toThrow(/protocol max/)
  })

  it('accepts the largest in-range value', () => {
    const justUnder = new Decimal(WIRE_MAX_SCALED.toString()).div('1e8').minus('0.00000001')
    expect(() => toWireSize(justUnder)).not.toThrow()
  })

  it('round-trips Decimal -> wire -> Decimal', () => {
    const cases = ['0', '1', '3000.5', '0.12345678', '10000000000']
    for (const c of cases) {
      const d = new Decimal(c)
      expect(fromWireSize(toWireSize(d)).equals(d)).toBe(true)
      expect(fromWirePrice(toWirePrice(d)).equals(d)).toBe(true)
    }
  })

  it('fromWireSize rejects exponential strings', () => {
    expect(() => fromWireSize('1e3')).toThrow(/exponential/)
    expect(() => fromWireSize('1.5E2')).toThrow(/exponential/)
  })

  it('fromWirePrice rejects negatives and excess precision', () => {
    expect(() => fromWirePrice('-1')).toThrow(/non-negative/)
    expect(() => fromWirePrice('0.000000001')).toThrow(/8dp/)
  })

  it('fromWire helpers require strings, not numbers', () => {
    // @ts-expect-error: intentional misuse to ensure runtime guard fires
    expect(() => fromWireSize(123)).toThrow(TypeError)
  })
})

describe('on-chain conversion', () => {
  it('encodes USDC (6 dp) correctly', () => {
    expect(toOnchainAmount('3000.5', TOKEN_DECIMALS.USDC)).toBe(3_000_500_000n)
    expect(toOnchainAmount('0', TOKEN_DECIMALS.USDC)).toBe(0n)
    expect(toOnchainAmount('0.000001', TOKEN_DECIMALS.USDC)).toBe(1n) // 1 USDC base unit
  })

  it('encodes WETH (18 dp) correctly', () => {
    expect(toOnchainAmount('1', TOKEN_DECIMALS.WETH)).toBe(1_000_000_000_000_000_000n)
    expect(toOnchainAmount('0.5', TOKEN_DECIMALS.WETH)).toBe(500_000_000_000_000_000n)
    expect(toOnchainAmount('0.000000000000000001', TOKEN_DECIMALS.WETH)).toBe(1n)
  })

  it('encodes price field per ADR (× quote decimals)', () => {
    // For ETH/USDC, m.price on-chain = price_decimal × 1e6 (quote decimals).
    expect(toOnchainAmount('3000', TOKEN_DECIMALS.USDC)).toBe(3_000_000_000n)
  })

  it('rejects negatives', () => {
    expect(() => toOnchainAmount('-1', 18)).toThrow(/non-negative/)
  })

  it('rejects excess precision beyond token decimals', () => {
    // USDC has 6dp; 0.0000001 is 7dp → must reject (not silently truncate).
    expect(() => toOnchainAmount('0.0000001', TOKEN_DECIMALS.USDC)).toThrow(/exceeds 6dp/)
  })

  it('rejects invalid decimals scalar', () => {
    expect(() => toOnchainAmount('1', -1)).toThrow()
    expect(() => toOnchainAmount('1', 1.5)).toThrow()
    expect(() => toOnchainAmount('1', 31)).toThrow()
  })

  it('decodes USDC and WETH on-chain amounts', () => {
    expect(fromOnchainAmount(3_000_500_000n, TOKEN_DECIMALS.USDC).toString()).toBe('3000.5')
    expect(fromOnchainAmount(500_000_000_000_000_000n, TOKEN_DECIMALS.WETH).toString()).toBe('0.5')
    expect(fromOnchainAmount(0n, TOKEN_DECIMALS.WETH).toString()).toBe('0')
  })

  it('requires a bigint for fromOnchainAmount', () => {
    // @ts-expect-error: intentional misuse to ensure runtime guard fires
    expect(() => fromOnchainAmount(100, 18)).toThrow(TypeError)
  })

  it('round-trips Decimal -> on-chain -> Decimal at full token precision', () => {
    const cases: Array<[string, number]> = [
      ['3000.5', TOKEN_DECIMALS.USDC],
      ['0.123456', TOKEN_DECIMALS.USDC],
      ['1.000000000000000001', TOKEN_DECIMALS.WETH],
      ['0', TOKEN_DECIMALS.WETH],
    ]
    for (const [s, dp] of cases) {
      const big = toOnchainAmount(s, dp)
      expect(fromOnchainAmount(big, dp).toString()).toBe(new Decimal(s).toString())
    }
  })
})

describe('display formatters', () => {
  it('formatPrice pads to default 2dp and rounds half-up', () => {
    expect(formatPrice('3000')).toBe('3000.00')
    expect(formatPrice('3000.1')).toBe('3000.10')
    expect(formatPrice('3000.125')).toBe('3000.13')
    expect(formatPrice('3000.124')).toBe('3000.12')
  })

  it('formatPrice honors custom dp', () => {
    expect(formatPrice('3000.123456', 4)).toBe('3000.1235')
    expect(formatPrice('3000.5', 0)).toBe('3001')
  })

  it('formatSize pads to default 4dp', () => {
    expect(formatSize('1')).toBe('1.0000')
    expect(formatSize('0.12345')).toBe('0.1235') // half-up
    expect(formatSize('0.12344')).toBe('0.1234')
  })

  it('formatSize honors custom dp', () => {
    expect(formatSize('1.23456789', 8)).toBe('1.23456789')
  })

  it('rejects invalid display dp', () => {
    expect(() => formatPrice('1', -1)).toThrow()
    expect(() => formatSize('1', 1.5)).toThrow()
  })
})

describe('constants', () => {
  it('WIRE_MAX_DP mirrors the ZK encoder', () => {
    expect(WIRE_MAX_DP).toBe(8)
  })

  it('WIRE_MAX_SCALED mirrors the ZK encoder 2^60 ceiling', () => {
    expect(WIRE_MAX_SCALED).toBe(1n << 60n)
  })

  it('TOKEN_DECIMALS matches ERC-20 reality', () => {
    expect(TOKEN_DECIMALS.WETH).toBe(18)
    expect(TOKEN_DECIMALS.USDC).toBe(6)
  })
})
