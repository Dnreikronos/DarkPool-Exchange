import * as React from 'react'

import { walletStore } from '../../../lib/wallet/mock-store'
import { BalancesPanel } from './BalancesPanel'

// Ladle is the project's visual-verification surface (the JSX test
// transform is node-only). Each story imperatively positions the wallet
// mock store before render; Ladle renders stories in their own iframe so
// per-story side effects don't leak.

function useWalletInitialState(connect: boolean) {
  React.useEffect(() => {
    if (connect) walletStore.connect()
    else walletStore.disconnect()
    return () => {
      walletStore.disconnect()
    }
  }, [connect])
}

export const Disconnected = () => {
  useWalletInitialState(false)
  return (
    <div className="max-w-md">
      <BalancesPanel />
    </div>
  )
}

export const Connected = () => {
  useWalletInitialState(true)
  return (
    <div className="max-w-md">
      <BalancesPanel />
    </div>
  )
}

export const InTwoColumnDock = () => {
  // The panel is designed to sit in a left-rail dock above order entry,
  // approximately ~280px wide. This story previews that constraint.
  useWalletInitialState(true)
  return (
    <div className="w-[280px]">
      <BalancesPanel />
    </div>
  )
}
