import type {
  AuctionSummary,
  GetAuctionHistoryResponse,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

/**
 * Pulls the `auctions` array out of a `useAuctionHistory` query result
 * and falls back to an empty list while the first request is in flight.
 * Lives in `_lib/` (not next to the hook) so unit tests can exercise it
 * without booting the SDK provider and its env-validated config module.
 */
export function auctionsFromQuery(
  query: Pick<{ data?: GetAuctionHistoryResponse }, 'data'>
): readonly AuctionSummary[] {
  return query.data?.auctions ?? []
}
