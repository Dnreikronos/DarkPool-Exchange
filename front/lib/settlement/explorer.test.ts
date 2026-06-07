import { describe, expect, it } from 'vitest'
import { arbitrum, foundry } from 'wagmi/chains'

import { settlementLink, shortTxHash, txExplorerUrl } from './explorer'

const TX = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

describe('txExplorerUrl', () => {
  it('builds a /tx/ URL from the chain block explorer', () => {
    expect(txExplorerUrl(TX, arbitrum)).toBe(`https://arbiscan.io/tx/${TX}`)
  })

  it('returns null when the chain has no block explorer (local devnet)', () => {
    expect(txExplorerUrl(TX, foundry)).toBeNull()
  })
})

describe('settlementLink', () => {
  const event = { batchId: '0xb1', txHash: TX, timestampUnix: 1n }

  it('projects an event into a view link with explorer URL', () => {
    expect(settlementLink(event, arbitrum)).toEqual({
      txHash: TX,
      url: `https://arbiscan.io/tx/${TX}`,
    })
  })

  it('keeps the hash with a null URL on explorerless chains', () => {
    expect(settlementLink(event, foundry)).toEqual({ txHash: TX, url: null })
  })

  it('passes through missing events as null', () => {
    expect(settlementLink(undefined, arbitrum)).toBeNull()
    expect(settlementLink(null, arbitrum)).toBeNull()
  })
})

describe('shortTxHash', () => {
  it('truncates to 0x-prefixed head and tail', () => {
    expect(shortTxHash(TX)).toBe('0xdead…beef')
  })

  it('leaves short strings untouched', () => {
    expect(shortTxHash('0xabc')).toBe('0xabc')
  })
})
