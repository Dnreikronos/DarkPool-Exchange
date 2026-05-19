'use client'

import { NumericText } from '../../NumericText'

import type { FormattedDelta } from './depth'

export interface OrderBookHeaderProps {
  /** Latest clearing price as a wire-string. `null` while no auctions have run yet. */
  clearingPrice: string | null
  /** Signed delta vs the previous auction. `null` mirrors `clearingPrice == null`. */
  delta: FormattedDelta | null
}

/**
 * Top of the orderbook column: the latest auction's clearing price + its
 * signed delta against the prior auction. Renders a placeholder when no
 * auction has been observed yet. Per DESIGN, the delta has no semantic
 * hue — sign is conveyed by the leading `+` / `-` and a luminance step.
 */
export function OrderBookHeader({ clearingPrice, delta }: OrderBookHeaderProps) {
  const hasPrice = clearingPrice !== null
  const deltaTone =
    delta?.sign === 'pos' || delta?.sign === 'zero' ? 'text-brand-fg' : 'text-brand-muted'

  return (
    <div className="flex flex-col items-center gap-1 border-b border-brand-border px-4 py-4">
      {hasPrice ? (
        <NumericText
          value={clearingPrice}
          kind="price"
          align="right"
          className="font-display text-display-sm leading-none tracking-tight"
          aria-label="Last clearing price"
        />
      ) : (
        <span
          className="font-display text-display-sm leading-none text-brand-muted"
          aria-label="No clearing price yet"
        >
          —
        </span>
      )}
      <span className="font-mono text-label-md uppercase text-brand-muted" aria-live="polite">
        [ LAST{delta && delta.sign !== 'na' ? ' · Δ ' : ' · '}
        <span className={deltaTone}>{delta?.text ?? '—'}</span>
        {' ]'}
      </span>
    </div>
  )
}
