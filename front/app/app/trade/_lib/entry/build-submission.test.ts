import { describe, expect, it, vi } from 'vitest'

import type { DecryptedOrderPayload } from '@/lib/crypto'

import { buildOrderPayload, buildWitness, createRealSteps, randomHex } from './build-submission'
import { ORDER_PAIR, ORDER_TTL_NS } from './policy'

const TRADER = '0x1234567890123456789012345678901234567890'

describe('randomHex', () => {
  it('returns 2*n lowercase hex chars', () => {
    const hex = randomHex(32)
    expect(hex).toMatch(/^[0-9a-f]{64}$/)
  })
  it('returns a different value each call', () => {
    expect(randomHex(32)).not.toBe(randomHex(32))
  })
})

describe('buildWitness', () => {
  it('maps buy→0 and carries the hex key + salt and string price/size', () => {
    const w = buildWitness({
      commitmentKey: 'aa'.repeat(32),
      saltHex: 'bb'.repeat(32),
      side: 'buy',
      price: '3000.5',
      size: '0.25',
    })
    expect(w).toEqual({
      commitment_key: 'aa'.repeat(32),
      side: 0,
      price: '3000.5',
      size: '0.25',
      salt_hex: 'bb'.repeat(32),
    })
  })
  it('maps sell→1', () => {
    expect(buildWitness({ commitmentKey: 'aa', saltHex: 'bb', side: 'sell', price: '1', size: '1' }).side).toBe(1)
  })
})

describe('buildOrderPayload', () => {
  it('produces the DecryptedOrder shape with side as 0|1 and ttl in ns', () => {
    const p: DecryptedOrderPayload = buildOrderPayload({
      trader: TRADER,
      pair: ORDER_PAIR,
      side: 'sell',
      price: '3000.5',
      size: '0.25',
      commitmentKey: 'aa'.repeat(32),
      ttlNs: ORDER_TTL_NS,
    })
    expect(p).toEqual({
      trader: TRADER,
      pair: 'ETH/USDC',
      side: 1,
      price: '3000.5',
      size: '0.25',
      commitment_key: 'aa'.repeat(32),
      ttl: 300_000_000_000,
    })
  })
})

describe('createRealSteps', () => {
  function deps(overrides = {}) {
    return {
      trader: TRADER,
      pair: ORDER_PAIR,
      ttlNs: ORDER_TTL_NS,
      side: 'buy' as const,
      price: '3000',
      size: '0.5',
      randomHex: vi
        .fn<(nBytes: number) => string>()
        .mockReturnValueOnce('cc'.repeat(32)) // commitment_key
        .mockReturnValueOnce('dd'.repeat(32)), // salt_hex
      getOperatorPubkey: vi.fn(() => new Uint8Array([0x04, 0x01])),
      prove: vi.fn(async () => ({ proof: new Uint8Array([1]), commitment: new Uint8Array([2]) })),
      serialize: vi.fn<(payload: DecryptedOrderPayload) => Uint8Array>(() => new Uint8Array([9, 9])),
      encrypt: vi.fn(() => new Uint8Array([7, 7])),
      placeOrder: vi.fn(async () => undefined),
      ...overrides,
    }
  }

  const ctx = { aborted: () => false }

  it('emits four steps in pipeline order', () => {
    const steps = createRealSteps(deps())
    expect(steps.map((s) => s.id)).toEqual(['preparing', 'proving', 'encrypting', 'submitting'])
  })

  it('threads ONE commitment_key into both witness and payload', async () => {
    const d = deps()
    const steps = createRealSteps(d)
    await steps[0].run(ctx) // preparing
    await steps[1].run(ctx) // proving
    await steps[2].run(ctx) // encrypting

    // witness passed to prove
    expect(d.prove).toHaveBeenCalledWith(
      expect.objectContaining({ commitment_key: 'cc'.repeat(32), salt_hex: 'dd'.repeat(32), side: 0 })
    )
    // payload passed to serialize has the SAME commitment_key, no salt field
    const payload = d.serialize.mock.calls[0][0]
    expect(payload.commitment_key).toBe('cc'.repeat(32))
    expect(payload).not.toHaveProperty('salt_hex')
    expect(payload.trader).toBe(TRADER)
  })

  it('submits the prove output bytes through placeOrder', async () => {
    const d = deps()
    const steps = createRealSteps(d)
    for (const s of steps) await s.run(ctx)
    expect(d.encrypt).toHaveBeenCalledWith(new Uint8Array([9, 9]), new Uint8Array([0x04, 0x01]))
    expect(d.placeOrder).toHaveBeenCalledWith({
      commitment: new Uint8Array([2]),
      proof: new Uint8Array([1]),
      encryptedPayload: new Uint8Array([7, 7]),
    })
  })

  it('throws a clear error when the trader is missing', async () => {
    const steps = createRealSteps(deps({ trader: '' }))
    await expect(steps[0].run(ctx)).rejects.toThrow(/wallet/i)
  })
})
