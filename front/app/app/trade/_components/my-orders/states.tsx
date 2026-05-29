'use client'

import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from '@/components/ui/panel-state'

export function MyOrdersLoading({ rows = 4 }: { rows?: number }) {
  return <BoxSkeletonBlock rows={rows} cols={5} ariaLabel="Loading open orders" />
}

export function MyOrdersEmpty({ disconnected = false }: { disconnected?: boolean } = {}) {
  return <PanelEmpty label={disconnected ? '[ CONNECT WALLET ]' : '[ NO ORDERS YET ]'} />
}

export interface MyOrdersErrorProps {
  message?: string
  onRetry?: () => void
}

export function MyOrdersError({ message, onRetry }: MyOrdersErrorProps) {
  return <PanelError label="[ ORDERS UNAVAILABLE ]" message={message} onRetry={onRetry} />
}
