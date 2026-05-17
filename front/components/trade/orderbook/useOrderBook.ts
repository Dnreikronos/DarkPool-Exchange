'use client'

import { create } from '@bufbuild/protobuf'
import { keepPreviousData, useQuery, type UseQueryResult } from '@tanstack/react-query'

import { useDarkPoolClient } from '../../../lib/sdk/provider'
import { DEFAULT_PAIR } from '../../../lib/sdk/mocks/factories'
import {
  GetAuctionHistoryRequestSchema,
  GetOrderBookRequestSchema,
  type GetAuctionHistoryResponse,
  type GetOrderBookResponse,
} from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'

/**
 * 1000 ms = cadence of the F1.2 mock store's perturb tick. Faster polling
 * wastes ticks; slower polling drops mutations between refetches. When
 * I2.4 (#93) swaps the mock for the REST surface and I2.6 (#95) lands the
 * streaming bridge, refetchInterval drops to 0 and the stream takes over.
 */
export const ORDERBOOK_POLL_MS = 1000

export interface UseOrderBookOptions {
  pair?: string
  /** Override polling cadence — primarily for Storybook + tests. */
  refetchIntervalMs?: number
}

export function useOrderBook(opts: UseOrderBookOptions = {}): UseQueryResult<GetOrderBookResponse> {
  const client = useDarkPoolClient()
  const pair = opts.pair ?? DEFAULT_PAIR
  const refetchInterval = opts.refetchIntervalMs ?? ORDERBOOK_POLL_MS
  return useQuery({
    queryKey: ['darkpool', 'orderbook', pair],
    queryFn: () => client.getOrderBook(create(GetOrderBookRequestSchema, { pair })),
    refetchInterval,
    // Keep the prior snapshot visible during the in-flight refetch so the
    // table doesn't flash a loading state on every tick.
    placeholderData: keepPreviousData,
  })
}

export interface UseRecentAuctionsOptions {
  pair?: string
  /** Number of auctions to fetch. Default 2 (last + previous, for delta). */
  limit?: number
  refetchIntervalMs?: number
}

/**
 * Polls the last N auction summaries so the orderbook header can render the
 * latest clearing price and its delta vs the prior auction. The mock store
 * clears a new auction every 5 s; 1 s polling cheaply picks them up at the
 * top of the next tick.
 */
export function useRecentAuctions(
  opts: UseRecentAuctionsOptions = {}
): UseQueryResult<GetAuctionHistoryResponse> {
  const client = useDarkPoolClient()
  const pair = opts.pair ?? DEFAULT_PAIR
  const limit = opts.limit ?? 2
  const refetchInterval = opts.refetchIntervalMs ?? ORDERBOOK_POLL_MS
  return useQuery({
    queryKey: ['darkpool', 'auctions', pair, limit],
    queryFn: () =>
      client.getAuctionHistory(create(GetAuctionHistoryRequestSchema, { pair, limit })),
    refetchInterval,
    placeholderData: keepPreviousData,
  })
}
