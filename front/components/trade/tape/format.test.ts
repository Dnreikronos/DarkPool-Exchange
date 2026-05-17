import { describe, expect, it } from 'vitest'

import { formatRelativeTime } from './format'

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
