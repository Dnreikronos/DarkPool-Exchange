// Public surface for the Phase 1 mock layer. Panels and tests should
// import from '@/lib/sdk' (re-exported there) or this barrel — never
// reach into the individual modules.

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
  type Balances,
  type FactoryContext,
  type FactoryContextOptions,
  type Fill,
  type MockAuctionSummaryOptions,
  type MockBalancesOptions,
  type MockFillOptions,
  type MockOrderBookOptions,
  type MockOrderInfoOptions,
  type MockPriceLevelOptions,
} from './factories'

export {
  StoreMockClient,
  withMockPayload,
  type MockOrderPayload,
  type StoreMockClientOptions,
} from './client'
