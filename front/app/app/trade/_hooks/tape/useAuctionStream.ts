'use client'

import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import { useDarkPoolClient } from '@/lib/sdk/provider'
import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/sdk/client'
import {
  StreamAuctionsRequestSchema,
  type AuctionEvent,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { backoffDelay, type BackoffOptions } from '../../_lib/tape/backoff'

export type StreamConnectionStatus = 'connecting' | 'live' | 'degraded'

export interface UseAuctionStreamOptions {
  pair: string
  onEvent: (event: AuctionEvent) => void
  /** Called when the server reports it dropped events (broadcast lag). */
  onLag?: () => void
  /** Disable the stream (pure-polling mode); defaults to true. */
  enabled?: boolean
  /** Backoff tuning; defaults to full-jitter base 1 s / cap 30 s. */
  backoff?: BackoffOptions
}

// Auth / not-found / unimplemented mean reconnecting will not help — stop and
// let the REST poll surface the same error to the user.
const TERMINAL_CODES: ReadonlySet<number> = new Set([
  DARK_POOL_ERROR_CODES.UNAUTHENTICATED,
  DARK_POOL_ERROR_CODES.PERMISSION_DENIED,
  DARK_POOL_ERROR_CODES.NOT_FOUND,
  DARK_POOL_ERROR_CODES.UNIMPLEMENTED,
])

export function useAuctionStream(opts: UseAuctionStreamOptions): {
  status: StreamConnectionStatus
} {
  const { pair, enabled = true } = opts
  const client = useDarkPoolClient()
  const [status, setStatus] = React.useState<StreamConnectionStatus>('connecting')

  // Latest callbacks/config via refs so the effect doesn't re-subscribe (and
  // tear down the live connection) on every parent render.
  const onEventRef = React.useRef(opts.onEvent)
  const onLagRef = React.useRef(opts.onLag)
  const backoffRef = React.useRef(opts.backoff)
  onEventRef.current = opts.onEvent
  onLagRef.current = opts.onLag
  backoffRef.current = opts.backoff

  React.useEffect(() => {
    if (!enabled) return

    const abort = new AbortController()
    let attempt = 0
    let timer: ReturnType<typeof setTimeout> | null = null
    let stopped = false

    const schedule = (delayMs: number) => {
      timer = setTimeout(() => {
        void run()
      }, delayMs)
    }

    const run = async () => {
      if (stopped || abort.signal.aborted) return
      setStatus((s) => (s === 'live' ? s : 'connecting'))
      try {
        const stream = client.streamAuctions(create(StreamAuctionsRequestSchema, { pair }), {
          signal: abort.signal,
        })
        for await (const event of stream) {
          if (stopped) return
          attempt = 0
          setStatus('live')
          onEventRef.current(event)
        }
        // Graceful end (server/proxy closed) → degrade + backoff reconnect.
        if (stopped || abort.signal.aborted) return
        setStatus('degraded')
        schedule(backoffDelay(attempt++, backoffRef.current))
      } catch (err) {
        if (stopped || abort.signal.aborted) return
        if (err instanceof DarkPoolError && err.code === DARK_POOL_ERROR_CODES.DATA_LOSS) {
          // Events were dropped: the badge must leave LIVE (and let polling
          // resume) until the reconnect catches back up.
          setStatus('degraded')
          onLagRef.current?.()
          attempt = 0
          schedule(0) // link is healthy; reconnect immediately to catch up
          return
        }
        setStatus('degraded')
        if (err instanceof DarkPoolError && TERMINAL_CODES.has(err.code)) {
          return // stop retrying
        }
        schedule(backoffDelay(attempt++, backoffRef.current))
      }
    }

    void run()

    return () => {
      stopped = true
      if (timer) clearTimeout(timer)
      abort.abort()
    }
  }, [client, pair, enabled])

  return { status }
}
