// @vitest-environment jsdom
import { create } from '@bufbuild/protobuf'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

vi.mock('@/lib/config', () => ({ config: { useMocks: true, contracts: null, chainId: 31337 } }))

import { useAuctionFeed } from './useAuctionFeed'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import type { DarkPoolClient } from '@/lib/sdk/client'
import {
  AuctionEventSchema,
  AuctionSummarySchema,
  GetAuctionHistoryResponseSchema,
  type AuctionEvent,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function ev(id: string, ts: bigint): AuctionEvent {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '1',
    matchedVolume: '1',
    matchCount: 0,
    timestampUnix: ts,
  })
}

function historyResponse(ids: Array<[string, bigint]>) {
  return create(GetAuctionHistoryResponseSchema, {
    auctions: ids.map(([id, ts]) =>
      create(AuctionSummarySchema, {
        auctionId: id,
        pair: 'ETH/USDC',
        clearingPrice: '1',
        matchedVolume: '1',
        matchCount: 0,
        timestampUnix: ts,
      })
    ),
  })
}

interface FakeOpts {
  history?: ReturnType<typeof historyResponse>
  stream?: (signal?: AbortSignal) => AsyncIterable<AuctionEvent>
}

function fakeClient(opts: FakeOpts) {
  const getAuctionHistory = vi.fn(async () => opts.history ?? historyResponse([]))
  const stream =
    opts.stream ??
    (async function* () {
      await new Promise(() => {}) // block forever
    })
  // Production calls streamAuctions(request, { signal }); forward the abort
  // signal to the scripted generator (which expects it as its first arg).
  const streamAuctions = (_req: unknown, callOpts?: { signal?: AbortSignal }) =>
    stream(callOpts?.signal)
  const client = { getAuctionHistory, streamAuctions } as unknown as DarkPoolClient
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

describe('useAuctionFeed', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }))
  afterEach(() => vi.useRealTimers())

  it('merges history backfill with live stream events, newest first', async () => {
    async function* stream(signal?: AbortSignal) {
      yield ev('live2', 20n)
      await new Promise<void>((r) => signal?.addEventListener('abort', () => r(), { once: true }))
    }
    const { client } = fakeClient({ history: historyResponse([['hist1', 10n]]), stream })
    const { result } = renderHook(() => useAuctionFeed({ limit: 50 }), {
      wrapper: makeWrapper(client),
    })
    await waitFor(() =>
      expect(result.current.auctions.map((a) => a.auctionId)).toEqual(['live2', 'hist1'])
    )
    expect(result.current.status).toBe('live')
  })

  it('stops polling history once the stream is live', async () => {
    async function* stream(signal?: AbortSignal) {
      yield ev('live1', 20n)
      await new Promise<void>((r) => signal?.addEventListener('abort', () => r(), { once: true }))
    }
    const { client, getAuctionHistory } = fakeClient({ stream })
    const { result } = renderHook(() => useAuctionFeed({ limit: 50, refetchIntervalMs: 1000 }), {
      wrapper: makeWrapper(client),
    })
    await waitFor(() => expect(result.current.status).toBe('live'))
    const callsWhenLive = getAuctionHistory.mock.calls.length
    await vi.advanceTimersByTimeAsync(5000)
    expect(getAuctionHistory.mock.calls.length).toBe(callsWhenLive) // no further polls while live
  })
})
