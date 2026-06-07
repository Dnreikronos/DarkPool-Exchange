import { describe, it, expect, afterEach } from 'vitest'
import { buildSiweMessage, SIWE_STATEMENT } from './siwe-message'
import type { Address } from './types'

// A real EIP-55 checksummed address — viem's createSiweMessage validates checksum.
const ADDR: Address = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'

const ORIGINAL_WINDOW = (globalThis as { window?: unknown }).window
afterEach(() => {
  if (ORIGINAL_WINDOW === undefined) delete (globalThis as { window?: unknown }).window
  else Object.defineProperty(globalThis, 'window', { configurable: true, value: ORIGINAL_WINDOW })
})

describe('buildSiweMessage', () => {
  it('produces an EIP-4361 message with the given domain, address, uri, chain id and nonce', () => {
    const msg = buildSiweMessage({
      address: ADDR,
      chainId: 1,
      nonce: 'serverNonce123',
      domain: 'app.darkpool.exchange',
      uri: 'https://app.darkpool.exchange',
    })
    expect(msg).toContain('app.darkpool.exchange wants you to sign in with your Ethereum account:')
    expect(msg).toContain(ADDR)
    expect(msg).toContain('URI: https://app.darkpool.exchange')
    expect(msg).toContain('Version: 1')
    expect(msg).toContain('Chain ID: 1')
    expect(msg).toContain('Nonce: serverNonce123')
    expect(msg).toContain('Issued At:')
  })

  it('embeds the server-issued nonce verbatim (never a random one)', () => {
    const msg = buildSiweMessage({
      address: ADDR,
      chainId: 11155111,
      nonce: 'UNIQUEserverNonce',
      domain: 'localhost:3000',
      uri: 'http://localhost:3000',
    })
    const nonceLine = msg.split('\n').find((l) => l.startsWith('Nonce: '))
    expect(nonceLine).toBe('Nonce: UNIQUEserverNonce')
  })

  it('uses the default statement when none is provided', () => {
    const msg = buildSiweMessage({
      address: ADDR,
      chainId: 1,
      nonce: 'devNonce01',
      domain: 'localhost:3000',
      uri: 'http://localhost:3000',
    })
    expect(msg).toContain(SIWE_STATEMENT)
  })

  it('falls back to window.location host/origin when domain/uri are omitted', () => {
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: {
        location: { host: 'trade.darkpool.exchange', origin: 'https://trade.darkpool.exchange' },
      },
    })
    const msg = buildSiweMessage({ address: ADDR, chainId: 1, nonce: 'devNonce01' })
    expect(msg).toContain(
      'trade.darkpool.exchange wants you to sign in with your Ethereum account:'
    )
    expect(msg).toContain('URI: https://trade.darkpool.exchange')
  })
})
