import { describe, it, expect } from 'vitest'
import { computeSiweAction } from './siwe-action'
import type { Address } from './types'

const A: Address = '0xAAAAaaAAaaAAaaaAAAaAaaaAAaAAaaaaAaAAaaA1'
const B: Address = '0xBBBBbbBBbbBBbbbBBBbBbbbBBbBBbbbbBbBBbbB2'

describe('computeSiweAction', () => {
  it('signs in on a fresh connect with no existing session', () => {
    expect(computeSiweAction(null, 'connected', A, null)).toEqual({
      kind: 'sign-in',
      address: A,
    })
  })

  it('is a noop when a valid session already exists for the connected address (page refresh)', () => {
    expect(computeSiweAction(null, 'connected', A, A)).toEqual({ kind: 'noop' })
  })

  it('matches session vs wallet address case-insensitively (backend lowercases)', () => {
    const lower = A.toLowerCase() as Address
    expect(computeSiweAction(null, 'connected', A, lower)).toEqual({ kind: 'noop' })
  })

  it('signs in when the persisted session is for a different address than the wallet', () => {
    expect(computeSiweAction(null, 'connected', B, A)).toEqual({
      kind: 'sign-in',
      address: B,
    })
  })

  it('signs in for the new address on an account switch', () => {
    expect(computeSiweAction(A, 'connected', B, null)).toEqual({
      kind: 'sign-in',
      address: B,
    })
  })

  it('does NOT auto sign-in when the same address lost its session (expiry / 401 clear)', () => {
    // prev === next, no valid session: the user must re-sign manually, no surprise popup.
    expect(computeSiweAction(A, 'connected', A, null)).toEqual({ kind: 'noop' })
  })

  it('signs out on the transition into disconnected after being connected', () => {
    expect(computeSiweAction(A, 'disconnected', null, null)).toEqual({ kind: 'sign-out' })
  })

  it('is a noop on repeated disconnected ticks (never was connected)', () => {
    expect(computeSiweAction(null, 'disconnected', null, null)).toEqual({ kind: 'noop' })
  })

  it('is a noop while connecting', () => {
    expect(computeSiweAction(null, 'connecting', null, null)).toEqual({ kind: 'noop' })
  })

  it('is a noop while reconnecting', () => {
    expect(computeSiweAction(A, 'reconnecting', A, A)).toEqual({ kind: 'noop' })
  })
})
