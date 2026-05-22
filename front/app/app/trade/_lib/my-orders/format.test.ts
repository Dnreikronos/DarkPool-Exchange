import { describe, expect, it } from 'vitest'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { formatSubmittedAt, sideLabel, statusLabel } from './format'

describe('formatSubmittedAt', () => {
  it('renders a UNIX-second timestamp as HH:MM:SS in UTC', () => {
    // 2023-11-14T22:13:20Z
    expect(formatSubmittedAt(1700000000n)).toBe('22:13:20')
  })

  it('zero-pads single-digit components', () => {
    // 1970-01-01T00:05:09Z
    expect(formatSubmittedAt(309n)).toBe('00:05:09')
  })

  it('treats 0n as the unix epoch', () => {
    expect(formatSubmittedAt(0n)).toBe('00:00:00')
  })
})

describe('sideLabel', () => {
  it('renders BUY as a bracketed tag', () => {
    expect(sideLabel(Side.BUY)).toBe('[ BUY ]')
  })

  it('renders SELL as a bracketed tag', () => {
    expect(sideLabel(Side.SELL)).toBe('[ SELL ]')
  })
})

describe('statusLabel', () => {
  it('renders each status as a bracketed uppercase tag', () => {
    expect(statusLabel('open')).toBe('[ OPEN ]')
    expect(statusLabel('filled')).toBe('[ FILLED ]')
    expect(statusLabel('cancelled')).toBe('[ CANCELLED ]')
  })
})
