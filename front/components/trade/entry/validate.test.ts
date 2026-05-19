import { describe, expect, it } from 'vitest'

import { validateOrder, type ValidationInput } from './validate'

const baseInput: ValidationInput = {
  side: 'buy',
  price: '3000',
  size: '0.5',
  baseBalance: '10',
  quoteBalance: '25000',
  isConnected: true,
}

describe('validateOrder', () => {
  it('reports ok for a valid buy with sufficient quote balance', () => {
    const result = validateOrder(baseInput)
    expect(result.ok).toBe(true)
    expect(result.errors).toEqual({})
  })

  it('reports ok for a valid sell with sufficient base balance', () => {
    const result = validateOrder({ ...baseInput, side: 'sell' })
    expect(result.ok).toBe(true)
  })

  it('flags wallet-disconnected before anything else', () => {
    const result = validateOrder({ ...baseInput, isConnected: false })
    expect(result.ok).toBe(false)
    expect(result.errors.form).toBe('wallet-disconnected')
  })

  it('flags missing price as price-required (not invalid)', () => {
    const result = validateOrder({ ...baseInput, price: '' })
    expect(result.ok).toBe(false)
    expect(result.errors.price).toBe('price-required')
  })

  it('flags missing size as size-required', () => {
    const result = validateOrder({ ...baseInput, size: '   ' })
    expect(result.ok).toBe(false)
    expect(result.errors.size).toBe('size-required')
  })

  it('flags non-numeric price as price-invalid', () => {
    const result = validateOrder({ ...baseInput, price: 'abc' })
    expect(result.errors.price).toBe('price-invalid')
  })

  it('flags zero/negative size as size-invalid', () => {
    expect(validateOrder({ ...baseInput, size: '0' }).errors.size).toBe('size-invalid')
    expect(validateOrder({ ...baseInput, size: '-1' }).errors.size).toBe('size-invalid')
  })

  it('flags price below MIN_PRICE', () => {
    const result = validateOrder({ ...baseInput, price: '0.0001' })
    expect(result.errors.price).toBe('price-below-min')
  })

  it('flags size below MIN_SIZE', () => {
    const result = validateOrder({ ...baseInput, size: '0.00001' })
    expect(result.errors.size).toBe('size-below-min')
  })

  it('flags insufficient quote balance on BUY (fee included)', () => {
    // price 3000 * size 8 = 24000; fee 12; grand 24012 > balance 24000.
    const result = validateOrder({
      ...baseInput,
      price: '3000',
      size: '8',
      quoteBalance: '24000',
    })
    expect(result.ok).toBe(false)
    expect(result.errors.form).toBe('insufficient-balance')
  })

  it('allows BUY when grand total equals balance exactly', () => {
    // price 100 * size 10 = 1000; fee 0.5; grand 1000.5.
    const result = validateOrder({
      ...baseInput,
      price: '100',
      size: '10',
      quoteBalance: '1000.5',
    })
    expect(result.ok).toBe(true)
  })

  it('flags insufficient base balance on SELL (size > base balance)', () => {
    const result = validateOrder({
      ...baseInput,
      side: 'sell',
      size: '20',
      baseBalance: '10',
    })
    expect(result.ok).toBe(false)
    expect(result.errors.form).toBe('insufficient-balance')
  })

  it('does not double-stack insufficient-balance on top of wallet-disconnected', () => {
    const result = validateOrder({
      ...baseInput,
      isConnected: false,
      quoteBalance: '0',
    })
    expect(result.errors.form).toBe('wallet-disconnected')
  })

  it('does not run balance check while a field error is still pending', () => {
    // Empty size — no balance check should run, no insufficient-balance.
    const result = validateOrder({ ...baseInput, size: '', quoteBalance: '0' })
    expect(result.errors.size).toBe('size-required')
    expect(result.errors.form).toBeUndefined()
  })
})
