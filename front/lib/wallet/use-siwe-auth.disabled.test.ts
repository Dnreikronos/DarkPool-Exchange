// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

const ADDR = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'
const signMessageAsync = vi.fn()

vi.mock('wagmi', () => ({
  useAccount: () => ({ address: ADDR, status: 'connected' }),
  useChainId: () => 31337,
  useSignMessage: () => ({ signMessageAsync }),
}))

// SIWE disabled (e.g. mock mode / non-SIWE deployment).
vi.mock('@/lib/config', () => ({
  config: { siweEnabled: false, useMocks: false, apiUrl: 'http://localhost:8080' },
}))

import { useSiweAuth } from './use-siwe-auth'
import { getSessionToken } from './session'

beforeEach(() => {
  signMessageAsync.mockReset()
  vi.stubGlobal('fetch', vi.fn())
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('useSiweAuth (SIWE disabled)', () => {
  it('signIn is inert — no signature prompt, no token', async () => {
    const { result } = renderHook(() => useSiweAuth())
    await act(async () => {
      await result.current.signIn()
    })
    expect(signMessageAsync).not.toHaveBeenCalled()
    expect(getSessionToken()).toBeNull()
  })

  it('isAuthenticated mirrors wallet connection so authed UI is not gated', () => {
    const { result } = renderHook(() => useSiweAuth())
    expect(result.current.isAuthenticated).toBe(true)
  })

  it('does not auto-sign-in even with autoSignIn set', () => {
    renderHook(() => useSiweAuth({ autoSignIn: true }))
    expect(signMessageAsync).not.toHaveBeenCalled()
  })
})
