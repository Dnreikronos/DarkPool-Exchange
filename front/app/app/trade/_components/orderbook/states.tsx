'use client'

const SKELETON_BAR = '████░░░░'

/**
 * Box-drawing skeleton — per DESIGN-INSPIRATIONS, loading uses skeleton
 * blocks (`█ ░`) rather than spinners. Each row's mask offset is fixed so
 * the skeleton reads as orderbook-shaped rather than uniform noise.
 */
export function OrderBookLoading({ rows = 8 }: { rows?: number }) {
  return (
    <div
      role="status"
      aria-label="Loading orderbook"
      aria-live="polite"
      className="flex flex-col gap-1 px-4 py-3"
    >
      {Array.from({ length: rows }).map((_, i) => (
        <div
          key={i}
          className="flex items-center justify-between font-mono text-body-sm text-brand-border2 animate-pulse"
          aria-hidden="true"
        >
          <span>{shift(SKELETON_BAR, i)}</span>
          <span>{shift(SKELETON_BAR, i + 3)}</span>
          <span>{shift(SKELETON_BAR, i + 5)}</span>
        </div>
      ))}
      <span className="sr-only">Loading orderbook…</span>
    </div>
  )
}

function shift(bar: string, by: number): string {
  const offset = ((by % bar.length) + bar.length) % bar.length
  return bar.slice(offset) + bar.slice(0, offset)
}

/**
 * Empty state — visually distinct from loading per the acceptance
 * criteria. Terse copy per DESIGN-INSPIRATIONS tone-of-copy table.
 */
export function OrderBookEmpty() {
  return (
    <div
      role="status"
      className="flex flex-1 items-center justify-center px-4 py-8 font-mono text-label-lg uppercase text-brand-muted"
    >
      [ NO ORDERS YET ]
    </div>
  )
}

export interface OrderBookErrorProps {
  message?: string
  onRetry?: () => void
}

export function OrderBookError({ message, onRetry }: OrderBookErrorProps) {
  return (
    <div
      role="alert"
      className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-8 text-center"
    >
      <span className="font-mono text-label-lg uppercase text-brand-muted">
        [ ORDERBOOK UNAVAILABLE ]
      </span>
      {message ? (
        <span className="max-w-xs font-mono text-body-sm text-brand-muted">{message}</span>
      ) : null}
      {onRetry ? (
        <button
          type="button"
          onClick={onRetry}
          className="border border-brand-border2 px-4 py-2 font-mono text-label-md uppercase text-brand-fg transition-colors hover:border-brand-accent hover:text-brand-accent focus-visible:border-brand-accent focus-visible:text-brand-accent focus-visible:outline-none"
        >
          [ RETRY ]
        </button>
      ) : null}
    </div>
  )
}
