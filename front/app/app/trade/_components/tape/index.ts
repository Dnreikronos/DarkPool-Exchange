// Public surface for the auction tape panel (F1.7 / issue #74).
// Consumers import { Tape } and mount it where the panel belongs in
// the trading shell. `TapeContent` is the provider-less variant for
// consumers that already have a `QueryClientProvider` ancestor (the
// trading layout does, via WalletProviders) — mirrors `OrderBookContent`.
export { Tape, TapeContent } from './Tape'
export type { TapeProps } from './Tape'
