import * as React from 'react'

import { walletStore } from '@/lib/wallet/mock-store'
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
  // Preview at a narrow ~280px width — useful for confirming the column
  // tags don't wrap when the panel docks into a sidebar. The actual
  // mounting location inside the trade shell is decided in a follow-up
  // PR, not this one.
  useWalletInitialState(true)
  return (
    <div className="w-[280px]">
      <BalancesPanel />
    </div>
  )
}
