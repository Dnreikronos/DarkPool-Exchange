import { describe, it, expect } from 'vitest'

import { buildBalanceContracts, mapBalanceResults } from './chain-reads'

const ADDRS = {
  darkPool: '0x1111111111111111111111111111111111111111',
  weth: '0x2222222222222222222222222222222222222222',
  usdc: '0x3333333333333333333333333333333333333333',
} as const
const TRADER = '0x4444444444444444444444444444444444444444' as const

describe('buildBalanceContracts', () => {
  it('builds 4 calls in order: DarkPool.balances(WETH,USDC) then ERC20.balanceOf(WETH,USDC)', () => {
    const calls = buildBalanceContracts(ADDRS, TRADER)
    expect(calls).toHaveLength(4)
    expect(calls[0]).toMatchObject({
      address: ADDRS.darkPool,
      functionName: 'balances',
      args: [TRADER, ADDRS.weth],
    })
    expect(calls[1]).toMatchObject({
      address: ADDRS.darkPool,
      functionName: 'balances',
      args: [TRADER, ADDRS.usdc],
    })
    expect(calls[2]).toMatchObject({
      address: ADDRS.weth,
      functionName: 'balanceOf',
      args: [TRADER],
    })
    expect(calls[3]).toMatchObject({
      address: ADDRS.usdc,
      functionName: 'balanceOf',
      args: [TRADER],
    })
  })
})

describe('mapBalanceResults', () => {
  it('formats raw bigints to decimal strings using per-token decimals (WETH 18dp, USDC 6dp)', () => {
    const out = mapBalanceResults([
      2_000000000000000000n, // internal WETH = 2
      1500_000000n, // internal USDC = 1500
      10_500000000000000000n, // wallet WETH = 10.5
      250_000000n, // wallet USDC = 250
    ])
    expect(out.internal).toEqual({ weth: '2', usdc: '1500' })
    expect(out.wallet).toEqual({ weth: '10.5', usdc: '250' })
  })
})
