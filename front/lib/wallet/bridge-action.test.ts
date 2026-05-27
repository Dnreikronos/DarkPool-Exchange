import { describe, expect, it } from 'vitest'
import { computeBridgeAction } from './bridge-action'
import type { Address } from './types'

const A: Address = '0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const B: Address = '0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'

describe('computeBridgeAction', () => {
  it('null → A (first connect): connect without clearing caches', () => {
    expect(computeBridgeAction(null, 'connected', A)).toEqual({
      kind: 'connect',
      address: A,
      clearCaches: false,
    })
  })

  it('A → A (reconnect after refresh): connect without clearing caches', () => {
    expect(computeBridgeAction(A, 'connected', A)).toEqual({
      kind: 'connect',
      address: A,
      clearCaches: false,
    })
  })

  it('A → B (account switch in the wallet): connect WITH cache clear', () => {
    expect(computeBridgeAction(A, 'connected', B)).toEqual({
      kind: 'connect',
      address: B,
      clearCaches: true,
    })
  })

  it('A → null (explicit disconnect): disconnect WITH cache clear', () => {
    expect(computeBridgeAction(A, 'disconnected', null)).toEqual({
      kind: 'disconnect',
      clearCaches: true,
    })
  })

  it('null → null (idle, never connected): disconnect WITHOUT cache clear', () => {
    expect(computeBridgeAction(null, 'disconnected', null)).toEqual({
      kind: 'disconnect',
      clearCaches: false,
    })
  })

  it('connecting / reconnecting are no-ops regardless of address', () => {
    expect(computeBridgeAction(null, 'connecting', null)).toEqual({ kind: 'noop' })
    expect(computeBridgeAction(A, 'reconnecting', A)).toEqual({ kind: 'noop' })
    expect(computeBridgeAction(A, 'reconnecting', B)).toEqual({ kind: 'noop' })
  })
})
