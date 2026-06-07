import { describe, expect, it } from 'vitest'

import { backoffDelay, BACKOFF_BASE_MS, BACKOFF_CAP_MS } from './backoff'

describe('backoffDelay', () => {
  it('attempt 0 draws from [0, base)', () => {
    expect(backoffDelay(0, { random: () => 0 })).toBe(0)
    expect(backoffDelay(0, { random: () => 0.999 })).toBe(Math.floor(0.999 * BACKOFF_BASE_MS))
    expect(backoffDelay(0, { random: () => 0.999 })).toBeLessThan(BACKOFF_BASE_MS)
  })

  it('ceiling doubles each attempt until the cap', () => {
    const half = (n: number) => backoffDelay(n, { random: () => 0.5 })
    expect(half(0)).toBe(BACKOFF_BASE_MS / 2) // 500
    expect(half(1)).toBe(BACKOFF_BASE_MS) // 1000
    expect(half(2)).toBe(BACKOFF_BASE_MS * 2) // 2000
    expect(half(10)).toBe(BACKOFF_CAP_MS / 2) // capped: 15000
  })

  it('never exceeds the cap, even with random→1', () => {
    const d = backoffDelay(50, { random: () => 0.999999 })
    expect(d).toBeLessThan(BACKOFF_CAP_MS)
  })

  it('treats negative attempts as 0', () => {
    expect(backoffDelay(-5, { random: () => 0 })).toBe(0)
  })
})
