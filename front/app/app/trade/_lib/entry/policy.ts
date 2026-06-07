// Policy constants for the order-entry form.
//
// The values here are Phase 1 defaults that future issues (#83 pair
// registry, #99 real placement) will replace by reading the live policy
// off the pair-registry RPC. Keep this file the single import point for
// every constant the form references so the swap stays a one-line change.

import type { TokenSymbol } from '@/lib/wallet/types'

/**
 * Protocol fee, in basis points. Mirrors `DarkPool.sol::PROTOCOL_FEE_BPS`
 * (5 bps = 0.05%). Quoted on the trader-paid side of the trade.
 */
export const FEE_BPS = 5

/** Lowest accepted limit price, in quote-token units (USDC). */
export const MIN_PRICE = '0.01'

/** Lowest accepted size, in base-token units (WETH). */
export const MIN_SIZE = '0.0001'

/**
 * Default ETH/USDC pair the form trades. Multi-pair is gated on backend
 * issue #29 — until then this is constant.
 */
export const BASE_TOKEN: TokenSymbol = 'WETH'
export const QUOTE_TOKEN: TokenSymbol = 'USDC'

/**
 * Stage durations for the mock submission pipeline. F1.12 (#79) reuses
 * the same stage IDs for onboarding copy, so renaming is a coordinated
 * change across both panels.
 */
export const STAGE_DURATIONS_MS = {
  preparing: 500,
  proving: 2000,
  encrypting: 200,
  submitting: 300,
} as const

export type SubmitStageId = keyof typeof STAGE_DURATIONS_MS

/**
 * Stage IDs in execution order. The progress bar weights each stage by
 * its duration so the bar moves at a constant pixel rate.
 */
export const STAGE_ORDER: readonly SubmitStageId[] = [
  'preparing',
  'proving',
  'encrypting',
  'submitting',
] as const

/** Total stage duration. Used by the progress bar normalisation. */
export const STAGE_TOTAL_MS = STAGE_ORDER.reduce((sum, id) => sum + STAGE_DURATIONS_MS[id], 0)

/**
 * Uppercase tracked labels for each stage. Matches the DESIGN.md tone:
 * informative, no exclamation, no marketing voice.
 */
export const STAGE_LABELS: Record<SubmitStageId, string> = {
  preparing: 'PREPARING WITNESS',
  proving: 'GENERATING PROOF',
  encrypting: 'ENCRYPTING',
  submitting: 'SUBMITTING',
}

/** Brief flash after a successful submission before the form resets. */
export const SUCCESS_HOLD_MS = 800

/**
 * Canonical pair string the operator's registry accepts. The engine
 * canonicalises to uppercase + slash (dp-types `Pair::parse`), and the
 * single seeded market is "ETH/USDC". Multi-pair is gated on #29.
 */
export const ORDER_PAIR = 'ETH/USDC'

/**
 * Order time-to-live, in NANOSECONDS. The engine reads `ttl` as a duration
 * in nanoseconds and sets `expires_at = now + ttl`
 * (dp-engine/src/engine.rs:551,557). 5 minutes keeps the order alive across
 * a few auction rounds without lingering.
 */
export const ORDER_TTL_NS = 5 * 60 * 1_000_000_000 // 300_000_000_000
