import { describe, expect, it } from 'vitest'
import type { Address } from './types'
import { normalizeTraderId } from './normalize'

describe('normalizeTraderId', () => {
  it('strips the 0x prefix and lowercases the remaining hex', () => {
    expect(normalizeTraderId('0x1111111111111111111111111111111111111111')).toBe(
      '1111111111111111111111111111111111111111'
    )
  })

  it('collapses EIP-55 mixed-case checksum addresses to lowercase', () => {
    expect(normalizeTraderId('0xAbCdEf0123456789AbCdEf0123456789AbCdEf01')).toBe(
      'abcdef0123456789abcdef0123456789abcdef01'
    )
  })

  it('returns exactly 40 ASCII hex characters', () => {
    const result = normalizeTraderId('0x1111111111111111111111111111111111111111')
    expect(result).toHaveLength(40)
    expect(/^[0-9a-f]{40}$/.test(result)).toBe(true)
  })

  it('throws on short address', () => {
    expect(() => normalizeTraderId('0x1234' as Address)).toThrow(/Invalid address/)
  })

  it('throws on missing 0x prefix', () => {
    expect(() => normalizeTraderId('1111111111111111111111111111111111111111' as Address)).toThrow(
      /Invalid address/
    )
  })

  it('throws on non-hex characters', () => {
    expect(() =>
      normalizeTraderId('0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ' as Address)
    ).toThrow(/Invalid address/)
  })
})
