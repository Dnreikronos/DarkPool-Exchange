'use client'

import * as React from 'react'

import { cn } from './cn'

const SKELETON_BAR = '████░░░░'

function shift(bar: string, by: number): string {
  const offset = ((by % bar.length) + bar.length) % bar.length
  return bar.slice(offset) + bar.slice(0, offset)
}

export interface BoxSkeletonRowProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Row index — drives the per-column shift so the block reads as data, not noise. */
  index?: number
  /** Number of bar segments rendered side-by-side. */
  cols?: number
}

/**
 * Single box-drawing skeleton row. The shifted offsets per column give the
 * row the silhouette of a populated table line rather than uniform noise —
 * DESIGN-INSPIRATIONS prescribes box-drawing skeletons over spinners.
 */
export function BoxSkeletonRow({ index = 0, cols = 3, className, ...rest }: BoxSkeletonRowProps) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        'flex items-center justify-between gap-3 font-mono text-body-sm text-brand-border2 animate-pulse motion-reduce:animate-none',
        className
      )}
      {...rest}
    >
      {Array.from({ length: cols }).map((_, c) => (
        <span key={c} className="whitespace-pre">
          {shift(SKELETON_BAR, index + c * 3)}
        </span>
      ))}
    </div>
  )
}

export interface BoxSkeletonBlockProps {
  rows?: number
  cols?: number
  /** Live label announced to assistive tech; defaults to `Loading`. */
  ariaLabel?: string
  className?: string
  rowClassName?: string
}

/**
 * Stack of `rows` box-drawing skeleton lines. Use this for tabular panels
 * (orderbook, tape, my-orders, portfolio fills). Charts that need a
 * rectangular shimmer should compose `<Skeleton>` from `./skeleton`
 * directly — it matches the design rule for non-tabular surfaces.
 */
export function BoxSkeletonBlock({
  rows = 6,
  cols = 3,
  ariaLabel = 'Loading',
  className,
  rowClassName,
}: BoxSkeletonBlockProps) {
  return (
    <div
      role="status"
      aria-label={ariaLabel}
      aria-live="polite"
      className={cn('flex flex-col gap-1 px-4 py-3', className)}
    >
      {Array.from({ length: rows }).map((_, i) => (
        <BoxSkeletonRow key={i} index={i} cols={cols} className={rowClassName} />
      ))}
      <span className="sr-only">{ariaLabel}…</span>
    </div>
  )
}

export interface PanelEmptyProps {
  /**
   * Bracketed-tag label (e.g. `[ NO ORDERS YET ]`). Caller is responsible
   * for the brackets so the wording reads naturally per-panel.
   */
  label: string
  /** Optional second line in `body-sm` mono. */
  hint?: string
  className?: string
}

export function PanelEmpty({ label, hint, className }: PanelEmptyProps) {
  return (
    <div
      role="status"
      className={cn(
        'flex flex-1 flex-col items-center justify-center gap-2 px-4 py-8 text-center',
        className
      )}
    >
      <span className="font-mono text-label-lg uppercase tracking-label text-brand-muted">
        {label}
      </span>
      {hint ? (
        <span className="max-w-xs font-mono text-body-sm text-brand-muted">{hint}</span>
      ) : null}
    </div>
  )
}

export interface PanelErrorProps {
  /** Bracketed-tag label (e.g. `[ ORDERBOOK UNAVAILABLE ]`). */
  label: string
  /** Optional message line in `body-sm`. */
  message?: string
  /** When provided, renders a ghost `[ RETRY ]` button that invokes it. */
  onRetry?: () => void
  /** Override the retry button label. Defaults to `[ RETRY ]`. */
  retryLabel?: string
  className?: string
}

export function PanelError({
  label,
  message,
  onRetry,
  retryLabel = '[ RETRY ]',
  className,
}: PanelErrorProps) {
  return (
    <div
      role="alert"
      className={cn(
        'flex flex-1 flex-col items-center justify-center gap-3 px-4 py-8 text-center',
        className
      )}
    >
      <span className="font-mono text-label-lg uppercase tracking-label text-brand-muted">
        {label}
      </span>
      {message ? (
        <span className="max-w-xs font-mono text-body-sm text-brand-muted">{message}</span>
      ) : null}
      {onRetry ? (
        <button
          type="button"
          onClick={onRetry}
          className="border border-brand-border2 px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-fg transition-colors hover:border-brand-accent hover:text-brand-accent focus-visible:border-brand-accent focus-visible:text-brand-accent focus-visible:outline-none"
        >
          {retryLabel}
        </button>
      ) : null}
    </div>
  )
}
