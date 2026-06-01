// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'

const ADDR = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'
const SIG = `0x${'ab'.repeat(65)}`

let account: { address?: string; status: string }
let chainId: number
const signMessageAsync = vi.fn<(args: { message: string }) => Promise<string>>()

vi.mock('wagmi', () => ({
  useAccount: () => account,
  useChainId: () => chainId,
  useSignMessage: () => ({ signMessageAsync }),
}))

vi.mock('@/lib/config', () => ({
  config: { siweEnabled: true, useMocks: false, apiUrl: 'http://localhost:8080' },
}))

import { useSiweAuth } from './use-siwe-auth'
import { clearSession, getSessionToken } from './session'

class MemoryStorage implements Storage {
  private map = new Map<string, string>()
  get length() {
    return this.map.size
  }
  clear() {
    this.map.clear()
  }
  getItem(k: string) {
    return this.map.has(k) ? (this.map.get(k) as string) : null
  }
  key(i: number) {
    return Array.from(this.map.keys())[i] ?? null
  }
  removeItem(k: string) {
    this.map.delete(k)
  }
  setItem(k: string, v: string) {
    this.map.set(k, v)
  }
}

beforeEach(() => {
  account = { address: undefined, status: 'disconnected' }
  chainId = 31337
  signMessageAsync.mockReset()
  signMessageAsync.mockResolvedValue(SIG)
  Object.defineProperty(window, 'localStorage', { configurable: true, value: new MemoryStorage() })
  clearSession()
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/v1/auth/nonce')) {
        return new Response(JSON.stringify({ nonce: 'serverNonce123' }), { status: 200 })
      }
      if (url.endsWith('/v1/auth/verify')) {
        return new Response(
          JSON.stringify({
            token: 'jwt-xyz',
            expires_at: Math.floor(Date.now() / 1000) + 3600,
            address: ADDR.toLowerCase(),
          }),
          { status: 200 }
        )
      }
      return new Response('{}', { status: 404 })
    })
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('useSiweAuth (SIWE enabled)', () => {
  it('signs in: fetch nonce -> sign EIP-4361 with server nonce -> verify -> store token', async () => {
    account = { address: ADDR, status: 'connected' }
    const { result } = renderHook(() => useSiweAuth())

    await act(async () => {
      await result.current.signIn()
    })

    expect(getSessionToken()).toBe('jwt-xyz')
    expect(result.current.isAuthenticated).toBe(true)
    expect(signMessageAsync).toHaveBeenCalledTimes(1)
    expect(signMessageAsync.mock.calls[0][0].message).toContain('Nonce: serverNonce123')
  })

  it('surfaces a rejected signature as an error without authenticating', async () => {
    account = { address: ADDR, status: 'connected' }
    signMessageAsync.mockRejectedValueOnce(new Error('User rejected the request'))
    const { result } = renderHook(() => useSiweAuth())

    await act(async () => {
      await result.current.signIn()
    })

    expect(getSessionToken()).toBeNull()
    expect(result.current.isAuthenticated).toBe(false)
    expect(result.current.error).toBeTruthy()
  })

  it('signOut clears the session', async () => {
    account = { address: ADDR, status: 'connected' }
    const { result } = renderHook(() => useSiweAuth())
    await act(async () => {
      await result.current.signIn()
    })
    expect(getSessionToken()).toBe('jwt-xyz')

    act(() => {
      result.current.signOut()
    })
    expect(getSessionToken()).toBeNull()
    expect(result.current.isAuthenticated).toBe(false)
  })

  it('auto-signs-in on connect when autoSignIn is set', async () => {
    account = { address: ADDR, status: 'connected' }
    renderHook(() => useSiweAuth({ autoSignIn: true }))
    await waitFor(() => expect(getSessionToken()).toBe('jwt-xyz'))
    expect(signMessageAsync).toHaveBeenCalled()
  })
})
