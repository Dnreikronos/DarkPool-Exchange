// @vitest-environment jsdom
import { create } from '@bufbuild/protobuf'
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as React from 'react'

// The SDK provider imports lib/config at module-eval time, which validates
// NEXT_PUBLIC_* env vars that aren't set under Vitest. Every test here passes
// an explicit client, so a static stub keeps the import chain happy without a
// real environment (same pattern as useAuctionHistory.test.tsx).
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

import { useAuctionStream } from './useAuctionStream'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import { DARK_POOL_ERROR_CODES, DarkPoolError, type DarkPoolClient } from '@/lib/sdk/client'
import { AuctionEventSchema, type AuctionEvent } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

function ev(id: string): AuctionEvent {
  return create(AuctionEventSchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '1',
    matchedVolume: '1',
    matchCount: 0,
    timestampUnix: 1n,
  })
}

type StreamFn = (signal: AbortSignal | undefined) => AsyncIterable<AuctionEvent>

// A DarkPoolClient whose streamAuctions plays the next scripted generator on
// each (re)connect, latching on the last one once the script is exhausted.
function scriptClient(streams: StreamFn[]): DarkPoolClient {
  let i = 0
  return {
    streamAuctions: (_req: unknown, opts?: { signal?: AbortSignal }) =>
      streams[Math.min(i++, streams.length - 1)](opts?.signal),
  } as unknown as DarkPoolClient
}

function makeWrapper(client: DarkPoolClient) {
  return function Wrapper({ children }: { children: React.ReactNode }): JSX.Element {
    return <DarkPoolClientProvider client={client}>{children}</DarkPoolClientProvider>
  }
}

// Yields the given events then blocks until the abort signal fires (a healthy,
// idle live connection).
async function* liveThenBlock(events: AuctionEvent[], signal?: AbortSignal) {
  for (const e of events) yield e
  await new Promise<void>((resolve) => {
    if (signal?.aborted) return resolve()
    signal?.addEventListener('abort', () => resolve(), { once: true })
  })
}

const ZERO_BACKOFF = { random: () => 0 }

describe('useAuctionStream', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }))
  afterEach(() => vi.useRealTimers())

  it('goes connecting → live and forwards events', async () => {
    const onEvent = vi.fn()
    const client = scriptClient([(signal) => liveThenBlock([ev('a1')], signal)])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(result.current.status).toBe('live'))
    expect(onEvent).toHaveBeenCalledTimes(1)
    expect(onEvent.mock.calls[0][0].auctionId).toBe('a1')
  })

  it('degrades then reconnects after a graceful end', async () => {
    const onEvent = vi.fn()
    // first connection: one event then ends; second: blocks live
    const client = scriptClient([
      async function* () {
        yield ev('a1')
      },
      (signal) => liveThenBlock([ev('a2')], signal),
    ])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    // graceful end of connection 1 → FSM reconnects → connection 2 yields a2
    await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(2))
    expect(onEvent.mock.calls[0][0].auctionId).toBe('a1')
    expect(onEvent.mock.calls[1][0].auctionId).toBe('a2')
    await waitFor(() => expect(result.current.status).toBe('live'))
  })

  it('calls onLag and reconnects immediately on DATA_LOSS', async () => {
    const onLag = vi.fn()
    const onEvent = vi.fn()
    const client = scriptClient([
      async function* () {
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.DATA_LOSS, 'lagged')
      },
      (signal) => liveThenBlock([ev('a2')], signal),
    ])
    renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, onLag, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(onLag).toHaveBeenCalledTimes(1))
    await vi.advanceTimersByTimeAsync(5)
    await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(1))
  })

  it('leaves live while reconnecting after DATA_LOSS', async () => {
    const onLag = vi.fn()
    const onEvent = vi.fn()
    const client = scriptClient([
      async function* () {
        yield ev('a1')
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.DATA_LOSS, 'lagged')
      },
      // Reconnect succeeds but has nothing to yield yet — the badge must not
      // still read LIVE (which would also keep polling suppressed).
      (signal) => liveThenBlock([], signal),
    ])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, onLag, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(onLag).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(result.current.status).not.toBe('live'))
  })

  it('stops retrying on a terminal auth error', async () => {
    const onEvent = vi.fn()
    let connects = 0
    const client = scriptClient([
      async function* () {
        connects++
        throw new DarkPoolError(DARK_POOL_ERROR_CODES.UNAUTHENTICATED, 'no key')
      },
    ])
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent, backoff: ZERO_BACKOFF }),
      { wrapper: makeWrapper(client) }
    )
    await waitFor(() => expect(result.current.status).toBe('degraded'))
    await vi.advanceTimersByTimeAsync(60_000)
    expect(connects).toBe(1) // no reconnect attempts
  })

  it('does not connect when disabled', async () => {
    const streamAuctions = vi.fn()
    const client = { streamAuctions } as unknown as DarkPoolClient
    const { result } = renderHook(
      () => useAuctionStream({ pair: 'ETH/USDC', onEvent: vi.fn(), enabled: false }),
      { wrapper: makeWrapper(client) }
    )
    await vi.advanceTimersByTimeAsync(50)
    expect(streamAuctions).not.toHaveBeenCalled()
    expect(result.current.status).toBe('connecting')
  })
})
