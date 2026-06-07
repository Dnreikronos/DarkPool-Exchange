// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

vi.mock('@/lib/config', () => ({
  config: {
    useMocks: true,
    apiUrl: 'http://localhost:8080',
    apiKey: 'test-key',
    chainId: 31337,
    operatorPubkeyUrl: 'http://localhost:8080/pubkey',
    contracts: null,
  },
}))

import { useAuctionHistory } from './useAuctionHistory'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import type { DarkPoolClient } from '@/lib/sdk/client'
import { create } from '@bufbuild/protobuf'
import { GetAuctionHistoryResponseSchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function clientWithSpy() {
  const getAuctionHistory = vi.fn(async () =>
    create(GetAuctionHistoryResponseSchema, { auctions: [] })
  )
  const client = { getAuctionHistory } as unknown as DarkPoolClient
  return { client, getAuctionHistory }
}

function makeWrapper(client: DarkPoolClient) {
  return function Wrapper({ children }: { children: React.ReactNode }): JSX.Element {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    return (
      <QueryClientProvider client={qc}>
        <DarkPoolClientProvider client={client}>{children}</DarkPoolClientProvider>
      </QueryClientProvider>
    )
  }
}

describe('useAuctionHistory polling toggle', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }))
  afterEach(() => vi.useRealTimers())

  it('does not poll again when refetchIntervalMs is false', async () => {
    const { client, getAuctionHistory } = clientWithSpy()
    renderHook(() => useAuctionHistory({ refetchIntervalMs: false }), {
      wrapper: makeWrapper(client),
    })
    await vi.advanceTimersByTimeAsync(0)
    await waitFor(() => expect(getAuctionHistory).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(10_000)
    expect(getAuctionHistory).toHaveBeenCalledTimes(1)
  })

  it('polls on the interval when given a number', async () => {
    const { client, getAuctionHistory } = clientWithSpy()
    renderHook(() => useAuctionHistory({ refetchIntervalMs: 1000 }), {
      wrapper: makeWrapper(client),
    })
    await vi.advanceTimersByTimeAsync(0)
    await waitFor(() => expect(getAuctionHistory).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(2500)
    expect(getAuctionHistory.mock.calls.length).toBeGreaterThanOrEqual(3)
  })
})
