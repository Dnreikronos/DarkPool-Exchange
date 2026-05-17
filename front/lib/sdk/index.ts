// Re-exports the typed wire schema generated from
// crates/dp-api/proto/darkpool/v1/darkpool.proto. Consumers should import
// from '@/lib/sdk' rather than reaching into 'proto/' directly so the
// generated path stays an implementation detail.
//
// Regenerate after any proto change: `npm run sdk:gen` (from front/).
//
// Wire-type gotchas:
// - Decimals (price, size, remainingSize, clearingPrice, matchedVolume,
//   totalSize) arrive as strings — never coerce to JS number.
// - int64 fields (submittedAtUnix, expiresAtUnix, timestampUnix) are
//   `bigint` on the message type and `string` on the *Json wire shape (per
//   grpc-gateway proto3 JSON mapping, mirrored in crates/dp-api/src/rest.rs).
//   Decode REST responses through the *Schema's fromJson() so the conversion
//   is explicit instead of relying on JSON.parse-then-cast.

export {
  // Order placement
  type PlaceOrderRequest,
  type PlaceOrderRequestJson,
  PlaceOrderRequestSchema,
  type PlaceOrderResponse,
  type PlaceOrderResponseJson,
  PlaceOrderResponseSchema,

  // Order cancellation
  type CancelOrderRequest,
  type CancelOrderRequestJson,
  CancelOrderRequestSchema,
  type CancelOrderResponse,
  type CancelOrderResponseJson,
  CancelOrderResponseSchema,

  // Order lookup
  type GetOrderRequest,
  type GetOrderRequestJson,
  GetOrderRequestSchema,
  type GetOrderResponse,
  type GetOrderResponseJson,
  GetOrderResponseSchema,

  // Order book
  type GetOrderBookRequest,
  type GetOrderBookRequestJson,
  GetOrderBookRequestSchema,
  type GetOrderBookResponse,
  type GetOrderBookResponseJson,
  GetOrderBookResponseSchema,
  type PriceLevel,
  type PriceLevelJson,
  PriceLevelSchema,

  // Auctions
  type GetAuctionHistoryRequest,
  type GetAuctionHistoryRequestJson,
  GetAuctionHistoryRequestSchema,
  type GetAuctionHistoryResponse,
  type GetAuctionHistoryResponseJson,
  GetAuctionHistoryResponseSchema,
  type AuctionSummary,
  type AuctionSummaryJson,
  AuctionSummarySchema,
  type StreamAuctionsRequest,
  type StreamAuctionsRequestJson,
  StreamAuctionsRequestSchema,
  type AuctionEvent,
  type AuctionEventJson,
  AuctionEventSchema,

  // Shared
  type OrderInfo,
  type OrderInfoJson,
  OrderInfoSchema,
  Side,
  type SideJson,
  SideSchema,

  // Service descriptor (for Connect/gRPC clients in later issues)
  DarkPoolService,
} from './proto/darkpool/v1/darkpool_pb.js'

// Friendly alias: the proto names the orderbook payload
// `GetOrderBookResponse`, but consumers think of it as the order book itself.
export type { GetOrderBookResponse as OrderBook } from './proto/darkpool/v1/darkpool_pb.js'
