'use client'

import { create } from '@bufbuild/protobuf'
import { keepPreviousData, useQuery, type UseQueryResult } from '@tanstack/react-query'

import { useDarkPoolClient } from '@/lib/sdk/provider'
import { DEFAULT_PAIR } from '@/lib/sdk/mocks/factories'
import {
  GetAuctionHistoryRequestSchema,
  type GetAuctionHistoryResponse,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

export const DEFAULT_AUCTION_HISTORY_LIMIT = 50

/**
 * 2000 ms matches the F1.7 spec and dp-engine's default 5 s auction cadence
 * — a poll every other second picks up the next clear within ~tickRate/2 of
 * the engine's commit. Tighter polling wastes ticks on an unchanged book;
 * looser polling lags the countdown the Tape header renders. I2.6 (#95)
 * replaces the poll with the SSE bridge and drops the interval to 0.
 */
export const AUCTION_HISTORY_POLL_MS = 2000

function normalizeLimit(limit: number): number {
  return Math.max(0, Math.floor(limit))
}

export interface UseAuctionHistoryOptions {
  pair?: string
  /** Max rows to request from the backend. Defaults to {@link DEFAULT_AUCTION_HISTORY_LIMIT}. */
  limit?: number
  /** Override polling cadence — primarily for Storybook + tests. */
  refetchIntervalMs?: number
}

/**
 * Polls `GET /v1/auctions` and returns the most recent auction summaries,
 * newest first. Flipped to the live REST surface in I2.4-style fashion by
 * setting `NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY=false`; otherwise the
 * factory routes the call to the in-memory `StoreMockClient`.
 *
 * Returns the raw {@link UseQueryResult} so callers can distinguish a
 * fresh-deployment empty list from a still-loading state. Consumers that
 * only care about the array can read `query.data?.auctions ?? []`.
 */
export function useAuctionHistory(
  opts: UseAuctionHistoryOptions = {}
): UseQueryResult<GetAuctionHistoryResponse> {
  const client = useDarkPoolClient()
  const pair = opts.pair ?? DEFAULT_PAIR
  const limit = normalizeLimit(opts.limit ?? DEFAULT_AUCTION_HISTORY_LIMIT)
  const refetchInterval = opts.refetchIntervalMs ?? AUCTION_HISTORY_POLL_MS
  return useQuery({
    queryKey: ['darkpool', 'auction-history', pair, limit],
    queryFn: () =>
      client.getAuctionHistory(create(GetAuctionHistoryRequestSchema, { pair, limit })),
    refetchInterval,
    placeholderData: keepPreviousData,
  })
}
