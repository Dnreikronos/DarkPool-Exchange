// Adapters between the per-token balance source and the display layer.
//
// The Phase 1 wallet mock (#70) hands the panel human-readable decimal
// strings ('1' for 1 WETH, '1000' for 1000 USDC) — the panel pipes those
// straight into NumericText with the per-token display dp picked here.
//
// Phase 2 (#91 wagmi codegen, #92 on-chain deposit/withdraw) replaces the
// mock with raw on-chain balances arriving as `bigint`. The swap path is
// `formatRawBalance(symbol, raw)` → decimal string → NumericText. This
// file keeps both shapes side-by-side so that swap stays a one-import
// change in BalancesPanel.tsx.

import { formatUnits } from 'viem'

import { TOKEN_DECIMALS, type TokenSymbol } from '@/lib/units'

const DISPLAY_DECIMALS: Readonly<Record<TokenSymbol, number>> = {
  WETH: 4,
  USDC: 2,
}

/**
 * Display precision for a token. Intentionally shallower than the
 * on-chain dp in {@link TOKEN_DECIMALS} — trading screens never surface
 * 18 dp of WETH or 6 dp of USDC. The cents-vs-fractional split (USDC=2,
 * WETH=4) matches the convention TradingView uses for USD-denominated
 * pairs.
 */
export function displayDecimalsFor(symbol: TokenSymbol): number {
  return DISPLAY_DECIMALS[symbol]
}

/**
 * Phase 2 path: convert a raw on-chain balance to a decimal string by
 * applying {@link TOKEN_DECIMALS} via viem's `formatUnits`. Returned as
 * a string so it can drop into NumericText without ever passing through
 * a JS number.
 */
export function formatRawBalance(symbol: TokenSymbol, raw: bigint): string {
  return formatUnits(raw, TOKEN_DECIMALS[symbol])
}
