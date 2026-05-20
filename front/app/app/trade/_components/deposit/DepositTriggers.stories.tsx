import * as React from 'react'

import { walletStore } from '@/lib/wallet/mock-store'

import { DepositTriggers } from './DepositTriggers'

function useWalletScenario({ connected, paused }: { connected: boolean; paused?: boolean }) {
  React.useEffect(() => {
    walletStore.resetTxState()
    if (connected) walletStore.connect()
    else walletStore.disconnect()
    if (paused) walletStore.setPaused(true)
    return () => {
      walletStore.disconnect()
      walletStore.resetTxState()
    }
  }, [connected, paused])
}

function StoryFrame({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-md bg-brand-surface">
      <div className="border border-brand-border">
        <div className="border-b border-brand-border p-3 font-mono text-label-md uppercase tracking-label text-brand-muted">
          {label}
        </div>
        {children}
      </div>
    </div>
  )
}

export const Disconnected = () => {
  useWalletScenario({ connected: false })
  return (
    <StoryFrame label="[ BALANCES ]">
      <DepositTriggers />
    </StoryFrame>
  )
}

export const Connected = () => {
  useWalletScenario({ connected: true })
  return (
    <StoryFrame label="[ BALANCES ]">
      <DepositTriggers />
    </StoryFrame>
  )
}

export const Paused = () => {
  useWalletScenario({ connected: true, paused: true })
  return (
    <StoryFrame label="[ BALANCES ]">
      <DepositTriggers />
    </StoryFrame>
  )
}

export const Compact = () => {
  useWalletScenario({ connected: true })
  return (
    <div className="mx-auto w-full max-w-md p-4">
      <DepositTriggers compact />
    </div>
  )
}
