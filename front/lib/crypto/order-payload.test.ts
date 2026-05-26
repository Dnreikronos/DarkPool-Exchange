import { describe, it, expect } from 'vitest'
import { serializeOrder, type DecryptedOrderPayload } from './order-payload'

const validPayload: DecryptedOrderPayload = {
  trader: '0x0000000000000000000000000000000000000000',
  pair: 'ETH-USD',
  side: 0,
  price: '2500.00',
  size: '1.0',
  commitment_key: 'abc123',
  ttl: 5_000_000_000,
}

describe('serializeOrder', () => {
  it('produces JSON matching Rust DecryptedOrder shape', () => {
    const bytes = serializeOrder(validPayload)
    const parsed = JSON.parse(new TextDecoder().decode(bytes))

    expect(parsed.trader).toBe('0x0000000000000000000000000000000000000000')
    expect(parsed.pair).toBe('ETH-USD')
    expect(parsed.side).toBe(0)
    expect(parsed.price).toBe('2500.00')
    expect(parsed.size).toBe('1.0')
    expect(parsed.commitment_key).toBe('abc123')
    expect(parsed.ttl).toBe(5_000_000_000)
  })

  it('side is an integer, not a string', () => {
    const bytes = serializeOrder({ ...validPayload, side: 1 })
    const parsed = JSON.parse(new TextDecoder().decode(bytes))
    expect(parsed.side).toStrictEqual(1)
    expect(typeof parsed.side).toBe('number')
  })

  it('price and size are strings, not numbers', () => {
    const bytes = serializeOrder(validPayload)
    const parsed = JSON.parse(new TextDecoder().decode(bytes))
    expect(typeof parsed.price).toBe('string')
    expect(typeof parsed.size).toBe('string')
  })

  it('uses snake_case commitment_key, not camelCase', () => {
    const bytes = serializeOrder(validPayload)
    const parsed = JSON.parse(new TextDecoder().decode(bytes))
    expect('commitment_key' in parsed).toBe(true)
    expect('commitmentKey' in parsed).not.toBe(true)
  })

  it('trader field is present and 0x-prefixed', () => {
    const bytes = serializeOrder(validPayload)
    const parsed = JSON.parse(new TextDecoder().decode(bytes))
    expect(parsed.trader).toBeDefined()
    expect(parsed.trader.startsWith('0x')).toBe(true)
  })

  it('rejects trader without 0x prefix', () => {
    expect(() =>
      serializeOrder({ ...validPayload, trader: '0000000000000000000000000000000000000000' })
    ).toThrow('0x-prefixed')
  })

  it('rejects trader with wrong length', () => {
    expect(() => serializeOrder({ ...validPayload, trader: '0xdead' })).toThrow('20-byte')
  })

  it('rejects empty price', () => {
    expect(() => serializeOrder({ ...validPayload, price: '' })).toThrow('price')
  })

  it('rejects empty size', () => {
    expect(() => serializeOrder({ ...validPayload, size: '' })).toThrow('size')
  })

  it('preserves field order matching Rust struct', () => {
    const bytes = serializeOrder(validPayload)
    const json = new TextDecoder().decode(bytes)
    const keys = Object.keys(JSON.parse(json))
    expect(keys).toEqual(['trader', 'pair', 'side', 'price', 'size', 'commitment_key', 'ttl'])
  })
})
