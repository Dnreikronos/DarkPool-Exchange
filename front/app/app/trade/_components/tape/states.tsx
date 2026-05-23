'use client'

import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from '@/components/ui/panel-state'

export function TapeLoading({ rows = 6 }: { rows?: number }) {
  return <BoxSkeletonBlock rows={rows} cols={4} ariaLabel="Loading auction tape" />
}

export function TapeEmpty() {
  return <PanelEmpty label="[ NO AUCTIONS YET ]" />
}

export interface TapeErrorProps {
  message?: string
  onRetry?: () => void
}

export function TapeError({ message, onRetry }: TapeErrorProps) {
  return <PanelError label="[ TAPE UNAVAILABLE ]" message={message} onRetry={onRetry} />
}
