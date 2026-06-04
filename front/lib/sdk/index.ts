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

// Order book — mock-only since #178 removed it from the wire. These are
// hand-written domain types, not generated from the proto. See ./orderbook.
export type { OrderBook, OrderBookRequest, PriceLevel } from './orderbook.js'

// ─── F1.2 mocks (#69) ────────────────────────────────────────────────────
//
// Re-exports the Phase 1 simulated backend: typed faker factories, the
// store-backed DarkPoolClient, and the runtime store handle. Phase 2
// (#90+) replaces these by flipping `useMocks=false` in createDarkPoolClient
// — no callsite changes required.

export {
  DEFAULT_MID,
  DEFAULT_PAIR,
  PRICE_DP,
  SIZE_DP,
  createFactoryContext,
  midFromBook,
  mockAuctionSummary,
  mockBalances,
  mockFill,
  mockOrderBook,
  mockOrderInfo,
  mockPriceLevel,
  scaleWireSize,
  StoreMockClient,
  withMockPayload,
  type Balances,
  type FactoryContext,
  type FactoryContextOptions,
  type Fill,
  type MockAuctionSummaryOptions,
  type MockBalancesOptions,
  type MockFillOptions,
  type MockOrderBookOptions,
  type MockOrderInfoOptions,
  type MockOrderPayload,
  type MockPriceLevelOptions,
  type StoreMockClientOptions,
} from './mocks/index.js'
