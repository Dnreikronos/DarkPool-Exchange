import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { clearPerTraderLocalStorage } from './per-trader-cache'

const ORIGINAL_LOCALSTORAGE = globalThis.localStorage

class MemoryStorage implements Storage {
  private store = new Map<string, string>()
  get length(): number {
    return this.store.size
  }
  clear(): void {
    this.store.clear()
  }
  getItem(key: string): string | null {
    return this.store.has(key) ? (this.store.get(key) as string) : null
  }
  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null
  }
  removeItem(key: string): void {
    this.store.delete(key)
  }
  setItem(key: string, value: string): void {
    this.store.set(key, String(value))
  }
}

describe('clearPerTraderLocalStorage', () => {
  beforeEach(() => {
    const memory = new MemoryStorage()
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: memory,
    })
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: { localStorage: memory },
    })
  })

  afterEach(() => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: ORIGINAL_LOCALSTORAGE,
    })
    // jsdom-less environments leave `window` undefined; restore that.
    delete (globalThis as { window?: unknown }).window
  })

  it('removes `dp:`-prefixed keys', () => {
    window.localStorage.setItem('dp:portfolio', '{"x":1}')
    window.localStorage.setItem('dp:orders:0xabc', '[]')
    clearPerTraderLocalStorage()
    expect(window.localStorage.getItem('dp:portfolio')).toBeNull()
    expect(window.localStorage.getItem('dp:orders:0xabc')).toBeNull()
  })

  it('leaves keys outside the per-trader namespace untouched', () => {
    window.localStorage.setItem('theme', 'dark')
    window.localStorage.setItem('dp:foo', 'bar')
    clearPerTraderLocalStorage()
    expect(window.localStorage.getItem('theme')).toBe('dark')
    expect(window.localStorage.getItem('dp:foo')).toBeNull()
  })

  it('is a no-op when window is undefined (SSR safety)', () => {
    delete (globalThis as { window?: unknown }).window
    expect(() => clearPerTraderLocalStorage()).not.toThrow()
  })
})
