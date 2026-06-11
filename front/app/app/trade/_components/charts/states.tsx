'use client'

import * as React from 'react'

import { cn } from '@/components/ui/cn'
import { PanelEmpty, PanelError } from '@/components/ui/panel-state'
import { Skeleton } from '@/components/ui/skeleton'

export interface ChartLoadingProps {
  className?: string
  ariaLabel?: string
}

/**
 * Rectangular shimmer for chart panels. Box-drawing skeletons only suit
 * tabular surfaces; a chart canvas reads better as a faint pulsing
 * block. The shimmer freezes under `prefers-reduced-motion`.
 */
export function ChartLoading({ className, ariaLabel = 'Loading chart' }: ChartLoadingProps = {}) {
  return (
    <div
      role="status"
      aria-label={ariaLabel}
      aria-live="polite"
      className={cn('flex h-full w-full items-stretch p-4', className)}
    >
      <Skeleton className="h-full w-full bg-brand-border" />
      <span className="sr-only">{ariaLabel}…</span>
    </div>
  )
}

export function DepthChartEmpty() {
  return <PanelEmpty label="[ NO BOOK YET ]" />
}

export function PriceChartEmpty() {
  return <PanelEmpty label="[ NO PRICE HISTORY ]" />
}

export interface ChartErrorProps {
  label?: string
  message?: string
  onRetry?: () => void
}

export function ChartError({
  label = '[ CHART UNAVAILABLE ]',
  message,
  onRetry,
}: ChartErrorProps = {}) {
  return <PanelError label={label} message={message} onRetry={onRetry} />
}
