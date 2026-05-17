import { describe, expect, it } from 'vitest'

import {
  formatRelativeTime,
  formatFullTimestamp,
  formatCount,
  secondsToNextAuction,
} from './format'

describe('formatRelativeTime', () => {
  it('renders a fresh auction as "0s"', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_000)).toBe('0s')
  })

  it('renders sub-minute ages in seconds', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_005)).toBe('5s')
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_059)).toBe('59s')
  })

  it('switches to minutes at and above 60s', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_000_060)).toBe('1m')
    expect(formatRelativeTime(1_700_000_000n, 1_700_003_599)).toBe('59m')
  })

  it('switches to hours at and above 60m', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_003_600)).toBe('1h')
    expect(formatRelativeTime(1_700_000_000n, 1_700_086_399)).toBe('23h')
  })

  it('switches to days at and above 24h', () => {
    expect(formatRelativeTime(1_700_000_000n, 1_700_086_400)).toBe('1d')
    expect(formatRelativeTime(1_700_000_000n, 1_700_259_200)).toBe('3d')
  })

  it('clamps negative ages (clock skew) to 0s', () => {
    expect(formatRelativeTime(1_700_000_010n, 1_700_000_000)).toBe('0s')
  })
})

describe('formatFullTimestamp', () => {
  it('renders HH:MM:SS · DD MMM YYYY in UTC', () => {
    // 1700000000 = 2023-11-14 22:13:20 UTC
    expect(formatFullTimestamp(1_700_000_000n)).toBe('22:13:20 · 14 NOV 2023')
  })

  it('zero-pads time components', () => {
    // 1672531200 = 2023-01-01 00:00:00 UTC
    expect(formatFullTimestamp(1_672_531_200n)).toBe('00:00:00 · 01 JAN 2023')
  })
})

describe('formatCount', () => {
  it('renders an integer as-is', () => {
    expect(formatCount(0)).toBe('0')
    expect(formatCount(1)).toBe('1')
    expect(formatCount(42)).toBe('42')
  })

  it('clamps negative inputs to 0', () => {
    expect(formatCount(-3)).toBe('0')
  })
})

describe('secondsToNextAuction', () => {
  it('returns intervalSeconds when no auction has landed yet', () => {
    expect(secondsToNextAuction(null, 1_700_000_000, 5)).toBe(5)
  })

  it('counts down from intervalSeconds after the last auction', () => {
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_000, 5)).toBe(5)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_002, 5)).toBe(3)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_004, 5)).toBe(1)
  })

  it('wraps to a fresh interval after the boundary passes (clock keeps moving past 5s when the mock is paused)', () => {
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_005, 5)).toBe(5)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_006, 5)).toBe(4)
    expect(secondsToNextAuction(1_700_000_000n, 1_700_000_012, 5)).toBe(3)
  })
})
