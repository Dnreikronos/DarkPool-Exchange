'use client'

import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from '@/components/ui/panel-state'

export function BalancesLoading({ rows = 2 }: { rows?: number }) {
  return <BoxSkeletonBlock rows={rows} cols={3} ariaLabel="Loading balances" />
}

/**
 * Disconnected hint. Balances do not have a "zero is empty" state — a
 * connected wallet with zero balances is still a real state worth
 * surfacing as `0.00`. This component covers the disconnected path
 * exclusively.
 */
export function BalancesDisconnected() {
  return <PanelEmpty label="[ CONNECT WALLET ]" />
}

export interface BalancesErrorProps {
  message?: string
  onRetry?: () => void
}

export function BalancesError({ message, onRetry }: BalancesErrorProps) {
  return <PanelError label="[ BALANCES UNAVAILABLE ]" message={message} onRetry={onRetry} />
}
