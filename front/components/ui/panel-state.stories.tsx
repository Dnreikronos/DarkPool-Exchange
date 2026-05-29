import * as React from 'react'

import { BoxSkeletonBlock, PanelEmpty, PanelError } from './panel-state'

export const SkeletonRows = () => (
  <div className="w-80 border border-brand-border bg-brand-surface">
    <BoxSkeletonBlock rows={6} cols={3} ariaLabel="Loading orderbook" />
  </div>
)

export const SkeletonWideRows = () => (
  <div className="w-[420px] border border-brand-border bg-brand-surface">
    <BoxSkeletonBlock rows={4} cols={4} ariaLabel="Loading fills" />
  </div>
)

export const Empty = () => (
  <div className="flex h-64 w-80 flex-col border border-brand-border bg-brand-surface">
    <PanelEmpty label="[ NO ORDERS YET ]" />
  </div>
)

export const EmptyWithHint = () => (
  <div className="flex h-64 w-80 flex-col border border-brand-border bg-brand-surface">
    <PanelEmpty label="[ NO FILLS YET ]" hint="Place an order to start your fill history." />
  </div>
)

export const Errored = () => (
  <div className="flex h-64 w-80 flex-col border border-brand-border bg-brand-surface">
    <PanelError label="[ ORDERBOOK UNAVAILABLE ]" message="Could not reach the engine." />
  </div>
)

export const ErroredWithRetry = () => (
  <div className="flex h-64 w-80 flex-col border border-brand-border bg-brand-surface">
    <PanelError
      label="[ ORDERBOOK UNAVAILABLE ]"
      message="Network refused the request."
      onRetry={() => console.info('retry')}
    />
  </div>
)
