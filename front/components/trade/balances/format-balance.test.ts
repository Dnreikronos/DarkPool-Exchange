import { describe, expect, it } from 'vitest'
import { formatUnits } from 'viem'

import { TOKEN_DECIMALS } from '../../../lib/units'

import { displayDecimalsFor, formatRawBalance } from './format-balance'

describe('displayDecimalsFor', () => {
  it.each([
    ['WETH', 4],
    ['USDC', 2],
  ] as const)('%s displays at %i dp', (symbol, dp) => {
    expect(displayDecimalsFor(symbol)).toBe(dp)
  })

  it('display dp is always shallower than the on-chain dp', () => {
    expect(displayDecimalsFor('WETH')).toBeLessThan(TOKEN_DECIMALS.WETH)
    expect(displayDecimalsFor('USDC')).toBeLessThan(TOKEN_DECIMALS.USDC)
  })
})

describe('formatRawBalance', () => {
  it('formats raw WETH using TOKEN_DECIMALS.WETH (18)', () => {
    expect(formatRawBalance('WETH', 1_000_000_000_000_000_000n)).toBe('1')
    expect(formatRawBalance('WETH', 1_500_000_000_000_000_000n)).toBe('1.5')
  })

  it('formats raw USDC using TOKEN_DECIMALS.USDC (6)', () => {
    expect(formatRawBalance('USDC', 1_000_000n)).toBe('1')
    expect(formatRawBalance('USDC', 1_500_000n)).toBe('1.5')
  })

  it('agrees with a direct formatUnits call', () => {
    const raw = 1_234_567n
    expect(formatRawBalance('WETH', raw)).toBe(formatUnits(raw, TOKEN_DECIMALS.WETH))
    expect(formatRawBalance('USDC', raw)).toBe(formatUnits(raw, TOKEN_DECIMALS.USDC))
  })

  it('renders zero as "0"', () => {
    expect(formatRawBalance('WETH', 0n)).toBe('0')
    expect(formatRawBalance('USDC', 0n)).toBe('0')
  })
})
