'use client'

import * as React from 'react'

import { DEFAULT_PAIR } from '@/lib/sdk/mocks/factories'
import type { AuctionEvent, AuctionSummary } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import {
  addLive,
  emptyFeed,
  mergeHistory,
  selectAuctions,
  type FeedState,
} from '../../_lib/tape/feed'
import {
  AUCTION_HISTORY_POLL_MS,
  DEFAULT_AUCTION_HISTORY_LIMIT,
  useAuctionHistory,
} from './useAuctionHistory'
import { useAuctionStream, type StreamConnectionStatus } from './useAuctionStream'

export interface UseAuctionFeedOptions {
  pair?: string
  limit?: number
  /** Storybook/test override for the degraded poll cadence. */
  refetchIntervalMs?: number
}

export interface AuctionFeed {
  auctions: AuctionSummary[]
  status: StreamConnectionStatus
}

type FeedAction =
  | { type: 'history'; auctions: readonly AuctionSummary[] }
  | { type: 'live'; event: AuctionEvent }

function feedReducer(state: FeedState, action: FeedAction): FeedState {
  switch (action.type) {
    case 'history':
      return mergeHistory(state, action.auctions)
    case 'live':
      return addLive(state, action.event)
  }
}

/**
 * The single data source the Tape consumes. Tries the live SSE stream first;
 * while it is connected the REST history poll is disabled, and on any drop the
 * poll resumes (seamless degrade) while the stream reconnects with backoff. A
 * server-reported lag triggers a one-shot history refetch to backfill the gap.
 */
export function useAuctionFeed(opts: UseAuctionFeedOptions = {}): AuctionFeed {
  const pair = opts.pair ?? DEFAULT_PAIR
  const limit = opts.limit ?? DEFAULT_AUCTION_HISTORY_LIMIT

  const [state, dispatch] = React.useReducer(feedReducer, undefined, emptyFeed)

  // onLag must call history.refetch, but history is created below; bounce
  // through a ref so the callback identity stays stable.
  const refetchRef = React.useRef<() => void>(() => {})
  const onEvent = React.useCallback((event: AuctionEvent) => dispatch({ type: 'live', event }), [])
  const onLag = React.useCallback(() => refetchRef.current(), [])

  const { status } = useAuctionStream({ pair, onEvent, onLag })

  const pollInterval: number | false =
    status === 'live' ? false : (opts.refetchIntervalMs ?? AUCTION_HISTORY_POLL_MS)
  const history = useAuctionHistory({ pair, limit, refetchIntervalMs: pollInterval })
  refetchRef.current = () => {
    void history.refetch()
  }

  const historyData = history.data
  React.useEffect(() => {
    if (historyData) dispatch({ type: 'history', auctions: historyData.auctions })
  }, [historyData])

  const auctions = React.useMemo(() => selectAuctions(state, limit), [state, limit])
  return { auctions, status }
}
