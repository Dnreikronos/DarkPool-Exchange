import { describe, expect, it } from 'vitest'

import { formatBatch, formatFillTimestamp } from './format'

describe('formatFillTimestamp', () => {
  it('renders a stable UTC timestamp', () => {
    // 1_700_000_000 → 2023-11-14T22:13:20Z
    expect(formatFillTimestamp(1_700_000_000n)).toBe('22:13:20 · 14 NOV')
  })

  it('zero-pads hours, minutes and seconds', () => {
    // 2023-01-01T00:00:00Z
    expect(formatFillTimestamp(1_672_531_200n)).toBe('00:00:00 · 01 JAN')
  })
})

describe('formatBatch', () => {
  it('returns short ids untouched', () => {
    expect(formatBatch('auc-001')).toBe('auc-001')
  })

  it('truncates long ids with an ellipsis', () => {
    expect(formatBatch('auc-abcdefghij')).toBe('auc-ab…ghij')
  })
})
