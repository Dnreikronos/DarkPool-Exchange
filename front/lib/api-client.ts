// Convenience re-exports for the trading app. Consumers should import from
// '@/lib/api-client' rather than reaching into '@/lib/sdk/*' so the SDK's
// internal layout (proto/, client.ts, mocks/, provider.tsx) stays an
// implementation detail.

export {
  createDarkPoolClient,
  DARK_POOL_ERROR_CODES,
  DARK_POOL_METHODS,
  DarkPoolError,
  MockClient,
  methodOverridesFromEnv,
  RestClient,
  type CreateDarkPoolClientOptions,
  type DarkPoolClient,
  type DarkPoolErrorCode,
  type DarkPoolErrorName,
  type DarkPoolMethod,
  type RestClientOptions,
  type StreamOptions,
} from './sdk/client'

export {
  DarkPoolClientProvider,
  useDarkPoolClient,
  type DarkPoolClientProviderProps,
} from './sdk/provider'
