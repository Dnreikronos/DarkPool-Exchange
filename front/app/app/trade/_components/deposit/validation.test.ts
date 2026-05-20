import { describe, expect, it } from 'vitest'
import { needsApproval, validateDeposit, validateWithdraw } from './validation'

describe('validateDeposit', () => {
  it('rejects empty input', () => {
    const r = validateDeposit({ amount: '', walletBalance: '1' })
    expect(r.ok).toBe(false)
    if (!r.ok) {
      expect(r.reason).toBe('empty')
      expect(r.message).toMatch(/Enter an amount/i)
    }
  })

  it('rejects whitespace-only input', () => {
    const r = validateDeposit({ amount: '   ', walletBalance: '1' })
    expect(r.ok).toBe(false)
    if (!r.ok) expect(r.reason).toBe('empty')
  })

  it('rejects malformed numbers', () => {
    const r = validateDeposit({ amount: 'banana', walletBalance: '1' })
    expect(r.ok).toBe(false)
    if (!r.ok) expect(r.reason).toBe('invalid')
  })

  it('rejects zero and negative amounts', () => {
    expect(validateDeposit({ amount: '0', walletBalance: '1' }).ok).toBe(false)
    expect(validateDeposit({ amount: '-1', walletBalance: '1' }).ok).toBe(false)
  })

  it('rejects when amount exceeds wallet balance', () => {
    const r = validateDeposit({ amount: '1.1', walletBalance: '1' })
    expect(r.ok).toBe(false)
    if (!r.ok) {
      expect(r.reason).toBe('exceeds-balance')
      expect(r.message).toMatch(/Insufficient wallet balance/i)
    }
  })

  it('accepts an amount equal to the wallet balance', () => {
    const r = validateDeposit({ amount: '1000', walletBalance: '1000' })
    expect(r.ok).toBe(true)
  })

  it('decimal math: 0.1 + 0.2 boundaries stay precise', () => {
    // 0.30000000000000004 in float — must not creep in.
    const r = validateDeposit({ amount: '0.3', walletBalance: '0.3' })
    expect(r.ok).toBe(true)
  })

  it('treats missing balance as zero', () => {
    const r = validateDeposit({ amount: '1', walletBalance: '' })
    expect(r.ok).toBe(false)
    if (!r.ok) expect(r.reason).toBe('exceeds-balance')
  })
})

describe('validateWithdraw', () => {
  it('mirrors deposit empty/invalid handling', () => {
    expect(validateWithdraw({ amount: '', internalBalance: '1' }).ok).toBe(false)
    expect(validateWithdraw({ amount: 'banana', internalBalance: '1' }).ok).toBe(false)
  })

  it('rejects when amount exceeds DarkPool balance', () => {
    const r = validateWithdraw({ amount: '1.1', internalBalance: '1' })
    expect(r.ok).toBe(false)
    if (!r.ok) {
      expect(r.reason).toBe('exceeds-balance')
      expect(r.message).toMatch(/Insufficient DarkPool balance/i)
    }
  })

  it('accepts when amount equals DarkPool balance', () => {
    const r = validateWithdraw({ amount: '500', internalBalance: '500' })
    expect(r.ok).toBe(true)
  })
})

describe('needsApproval', () => {
  it('returns true when allowance < amount', () => {
    expect(needsApproval('100', '50')).toBe(true)
  })

  it('returns false when allowance >= amount', () => {
    expect(needsApproval('100', '100')).toBe(false)
    expect(needsApproval('50', '100')).toBe(false)
  })

  it('handles empty inputs defensively (default zero)', () => {
    expect(needsApproval('', '')).toBe(false)
    expect(needsApproval('1', '')).toBe(true)
    expect(needsApproval('', '1')).toBe(false)
  })

  it('falls back to requiring approval on malformed input', () => {
    expect(needsApproval('banana', '100')).toBe(true)
  })
})
