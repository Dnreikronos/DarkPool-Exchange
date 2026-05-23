import { beforeEach, describe, expect, it } from 'vitest'

import {
  clearDismissed,
  isDismissed,
  promoteAnonToAddress,
  setDismissed,
  type StorageLike,
} from './storage'

function inMemoryStorage(): StorageLike {
  const map = new Map<string, string>()
  return {
    getItem: (k) => (map.has(k) ? (map.get(k) as string) : null),
    setItem: (k, v) => {
      map.set(k, v)
    },
    removeItem: (k) => {
      map.delete(k)
    },
  }
}

describe('onboarding storage', () => {
  let storage: StorageLike

  beforeEach(() => {
    storage = inMemoryStorage()
  })

  it('is not dismissed by default for any bucket', () => {
    expect(isDismissed(storage, null)).toBe(false)
    expect(isDismissed(storage, '0xabc')).toBe(false)
    expect(isDismissed(storage, '')).toBe(false)
  })

  it('persists dismissal for the connected address', () => {
    setDismissed(storage, '0xAbC123')
    expect(isDismissed(storage, '0xAbC123')).toBe(true)
  })

  it('treats addresses case-insensitively', () => {
    setDismissed(storage, '0xAbC123')
    expect(isDismissed(storage, '0xabc123')).toBe(true)
    expect(isDismissed(storage, '0xABC123')).toBe(true)
  })

  it('isolates buckets per address', () => {
    setDismissed(storage, '0xaaa')
    expect(isDismissed(storage, '0xaaa')).toBe(true)
    expect(isDismissed(storage, '0xbbb')).toBe(false)
  })

  it('uses an anon bucket when no wallet is connected', () => {
    setDismissed(storage, null)
    expect(isDismissed(storage, null)).toBe(true)
    expect(isDismissed(storage, '0xabc')).toBe(false)
  })

  it('clears the per-address flag', () => {
    setDismissed(storage, '0xabc')
    clearDismissed(storage, '0xabc')
    expect(isDismissed(storage, '0xabc')).toBe(false)
  })

  describe('promoteAnonToAddress', () => {
    it('copies an anon-bucket dismissal to the connecting address', () => {
      setDismissed(storage, null)
      promoteAnonToAddress(storage, '0xAbC')
      expect(isDismissed(storage, '0xAbC')).toBe(true)
    })

    it('does nothing when there is no anon dismissal', () => {
      promoteAnonToAddress(storage, '0xAbC')
      expect(isDismissed(storage, '0xAbC')).toBe(false)
    })

    it('does nothing for an empty address', () => {
      setDismissed(storage, null)
      promoteAnonToAddress(storage, '')
      // anon bucket survives, but no address bucket was touched.
      expect(isDismissed(storage, null)).toBe(true)
    })
  })
})
