// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement, type ReactNode } from 'react'
import { useOperatorPubkey } from './use-operator-pubkey'

function createWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client }, children)
  }
}

const fakePubkeyHex = '04' + 'ab'.repeat(64)

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ pubkey: fakePubkeyHex, encoding: 'sec1-uncompressed' }),
    }),
  )
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useOperatorPubkey', () => {
  it('fetches and decodes pubkey', async () => {
    const { result } = renderHook(() => useOperatorPubkey('http://localhost:8080'), {
      wrapper: createWrapper(),
    })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    const pubkey = result.current.data!
    expect(pubkey).toBeInstanceOf(Uint8Array)
    expect(pubkey.length).toBe(65)
    expect(pubkey[0]).toBe(0x04)
  })

  it('does not fetch when useMocks is true', async () => {
    const { result } = renderHook(() => useOperatorPubkey('http://localhost:8080', true), {
      wrapper: createWrapper(),
    })

    expect(result.current.fetchStatus).toBe('idle')
    expect(fetch).not.toHaveBeenCalled()
  })

  it('reports error on non-ok response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({ ok: false, status: 503 }),
    )

    const { result } = renderHook(() => useOperatorPubkey('http://localhost:8080'), {
      wrapper: createWrapper(),
    })

    await waitFor(() => expect(result.current.isError).toBe(true))
    expect(result.current.error?.message).toContain('503')
  })
})
