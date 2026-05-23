'use client'

import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from '@/components/ui/panel-state'

/**
 * Orderbook skeleton — three-column box-drawing block that matches the
 * Price / Size / Total layout of the populated rows. Driven by the shared
 * `BoxSkeletonBlock` primitive so all panels share the same shimmer
 * cadence and a11y wiring.
 */
export function OrderBookLoading({ rows = 8 }: { rows?: number }) {
  return <BoxSkeletonBlock rows={rows} cols={3} ariaLabel="Loading orderbook" />
}

export function OrderBookEmpty() {
  return <PanelEmpty label="[ NO ORDERS YET ]" />
}

export interface OrderBookErrorProps {
  message?: string
  onRetry?: () => void
}

export function OrderBookError({ message, onRetry }: OrderBookErrorProps) {
  return <PanelError label="[ ORDERBOOK UNAVAILABLE ]" message={message} onRetry={onRetry} />
}
