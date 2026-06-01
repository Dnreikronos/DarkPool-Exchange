import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import {
  SESSION_STORAGE_KEY,
  getSession,
  getSessionToken,
  setSession,
  clearSession,
  loadSession,
  subscribe,
  getSnapshot,
  type Session,
} from './session'
import type { Address } from './types'

const ADDR: Address = '0xAAAAaaAAaaAAaaaAAAaAaaaAAaAAaaaaAaAAaaA1'

// jsdom's localStorage is not relied upon here (it lacks a usable `clear`);
// follow the sibling per-trader-cache.test.ts pattern and install a
// Map-backed Storage on a stub `window` that session.ts reads.
class MemoryStorage implements Storage {
  private map = new Map<string, string>()
  get length(): number {
    return this.map.size
  }
  clear(): void {
    this.map.clear()
  }
  getItem(key: string): string | null {
    return this.map.has(key) ? (this.map.get(key) as string) : null
  }
  key(index: number): string | null {
    return Array.from(this.map.keys())[index] ?? null
  }
  removeItem(key: string): void {
    this.map.delete(key)
  }
  setItem(key: string, value: string): void {
    this.map.set(key, value)
  }
}

const ORIGINAL_WINDOW = (globalThis as { window?: unknown }).window
let memory: MemoryStorage

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000)
}

function makeSession(overrides: Partial<Session> = {}): Session {
  return { token: 'jwt-token', expiresAt: nowSeconds() + 3600, address: ADDR, ...overrides }
}

beforeEach(() => {
  memory = new MemoryStorage()
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { localStorage: memory },
  })
  clearSession()
})

afterEach(() => {
  if (ORIGINAL_WINDOW === undefined) {
    delete (globalThis as { window?: unknown }).window
  } else {
    Object.defineProperty(globalThis, 'window', { configurable: true, value: ORIGINAL_WINDOW })
  }
})

describe('session store', () => {
  it('returns null token when no session is set', () => {
    expect(getSessionToken()).toBeNull()
    expect(getSession()).toBeNull()
  })

  it('returns the token after setSession', () => {
    setSession(makeSession())
    expect(getSessionToken()).toBe('jwt-token')
    expect(getSession()?.address).toBe(ADDR)
  })

  it('persists the session to localStorage under the dp: prefixed key', () => {
    setSession(makeSession())
    expect(SESSION_STORAGE_KEY).toBe('dp:session-token')
    const raw = window.localStorage.getItem(SESSION_STORAGE_KEY)
    expect(raw).not.toBeNull()
    expect(JSON.parse(raw as string).token).toBe('jwt-token')
  })

  it('treats an expired session as no session (proactive expiry)', () => {
    setSession(makeSession({ expiresAt: nowSeconds() - 1 }))
    expect(getSessionToken()).toBeNull()
    expect(getSession()).toBeNull()
  })

  it('clearSession removes the session from memory and storage', () => {
    setSession(makeSession())
    clearSession()
    expect(getSessionToken()).toBeNull()
    expect(window.localStorage.getItem(SESSION_STORAGE_KEY)).toBeNull()
  })

  it('loadSession hydrates a valid persisted session', () => {
    window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(makeSession()))
    loadSession()
    expect(getSessionToken()).toBe('jwt-token')
  })

  it('loadSession drops and removes an expired persisted session', () => {
    window.localStorage.setItem(
      SESSION_STORAGE_KEY,
      JSON.stringify(makeSession({ expiresAt: nowSeconds() - 1 }))
    )
    loadSession()
    expect(getSessionToken()).toBeNull()
    expect(window.localStorage.getItem(SESSION_STORAGE_KEY)).toBeNull()
  })

  it('loadSession is a noop for malformed stored JSON (and clears the bad key)', () => {
    window.localStorage.setItem(SESSION_STORAGE_KEY, 'not-json{')
    loadSession()
    expect(getSessionToken()).toBeNull()
  })

  it('notifies subscribers on setSession and clearSession', () => {
    let calls = 0
    const unsub = subscribe(() => {
      calls += 1
    })
    setSession(makeSession())
    clearSession()
    unsub()
    setSession(makeSession())
    expect(calls).toBe(2)
  })

  it('getSnapshot returns a stable reference until the session changes', () => {
    const s = makeSession()
    setSession(s)
    const snap1 = getSnapshot()
    const snap2 = getSnapshot()
    expect(snap1).toBe(snap2)
    clearSession()
    expect(getSnapshot()).not.toBe(snap1)
  })
})
