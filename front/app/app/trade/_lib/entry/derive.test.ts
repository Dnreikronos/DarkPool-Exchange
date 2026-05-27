import { describe, expect, it } from 'vitest'

import { computeFee, computeGrandTotal, computeTotal } from './derive'
import { FEE_BPS } from './policy'

describe('computeTotal', () => {
  it('multiplies price * size as Decimal', () => {
    const total = computeTotal('3000', '0.5')
    expect(total).not.toBeNull()
    expect(total!.toFixed()).toBe('1500')
  })

  it('preserves precision past JS float (0.1 * 0.2)', () => {
    const total = computeTotal('0.1', '0.2')
    expect(total!.toFixed()).toBe('0.02')
  })

  it('returns null for empty inputs', () => {
    expect(computeTotal('', '1')).toBeNull()
    expect(computeTotal('1', '')).toBeNull()
    expect(computeTotal('  ', '1')).toBeNull()
  })

  it('returns null for non-numeric inputs', () => {
    expect(computeTotal('abc', '1')).toBeNull()
    expect(computeTotal('1', 'xyz')).toBeNull()
  })

  it('returns null when either side is negative', () => {
    expect(computeTotal('-1', '1')).toBeNull()
    expect(computeTotal('1', '-1')).toBeNull()
  })
})

describe('computeFee', () => {
  it('applies FEE_BPS = 5 (0.05%)', () => {
    expect(FEE_BPS).toBe(5)
    expect(computeFee('1000').toFixed()).toBe('0.5')
    expect(computeFee('2000').toFixed()).toBe('1')
  })

  it('survives small totals without rounding to zero', () => {
    // 5 bps of 0.01 = 0.000005 — keeps precision instead of clamping.
    expect(computeFee('0.01').toFixed(8)).toBe('0.00000500')
  })
})

describe('computeGrandTotal', () => {
  it('returns total + fee', () => {
    // price 3000 * size 0.5 = 1500; fee = 0.75; grand = 1500.75.
    expect(computeGrandTotal('3000', '0.5')!.toFixed()).toBe('1500.75')
  })

  it('returns null when inputs are unparseable', () => {
    expect(computeGrandTotal('', '0.5')).toBeNull()
    expect(computeGrandTotal('3000', '')).toBeNull()
    expect(computeGrandTotal('abc', '0.5')).toBeNull()
  })
})
