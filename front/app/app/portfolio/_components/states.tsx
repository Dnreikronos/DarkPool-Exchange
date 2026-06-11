'use client'

import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from '@/components/ui/panel-state'

export function PortfolioStatsLoading() {
  // Single line of 3 stat "values" — matches the WETH / USDC / P&L triplet.
  return <BoxSkeletonBlock rows={1} cols={3} ariaLabel="Loading portfolio summary" />
}

export function FillHistoryLoading({ rows = 4 }: { rows?: number }) {
  return <BoxSkeletonBlock rows={rows} cols={5} ariaLabel="Loading fill history" />
}

export function PortfolioDisconnected() {
  return <PanelEmpty label="[ CONNECT WALLET TO SEE POSITION + P&L ]" />
}

export function FillHistoryEmpty() {
  return (
    <PanelEmpty
      label="[ NO FILLS YET ]"
      hint="Place an order on /app/trade to start your history."
    />
  )
}

export interface PortfolioErrorProps {
  label?: string
  message?: string
  onRetry?: () => void
}

export function PortfolioError({
  label = '[ PORTFOLIO UNAVAILABLE ]',
  message,
  onRetry,
}: PortfolioErrorProps = {}) {
  return <PanelError label={label} message={message} onRetry={onRetry} />
}

export function FillHistoryError({
  label = '[ FILL HISTORY UNAVAILABLE ]',
  message,
  onRetry,
}: PortfolioErrorProps = {}) {
  return <PanelError label={label} message={message} onRetry={onRetry} />
}
