import * as React from 'react'

import { walletStore } from '@/lib/wallet/mock-store'

import { WithdrawForm } from './WithdrawForm'

function useWalletScenario({
  connected,
  paused,
  internal,
}: {
  connected: boolean
  paused?: boolean
  internal?: { weth?: string; usdc?: string }
}) {
  React.useEffect(() => {
    walletStore.resetTxState()
    if (connected) walletStore.connect()
    else walletStore.disconnect()
    if (internal?.weth) {
      walletStore.approve('WETH', internal.weth)
      walletStore.deposit('WETH', internal.weth)
    }
    if (internal?.usdc) {
      walletStore.approve('USDC', internal.usdc)
      walletStore.deposit('USDC', internal.usdc)
    }
    if (paused) walletStore.setPaused(true)
    return () => {
      walletStore.disconnect()
      walletStore.resetTxState()
    }
  }, [connected, paused, internal?.weth, internal?.usdc])
}

function StoryFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-md border border-brand-border bg-brand-surface p-8">
      {children}
    </div>
  )
}

export const Disconnected = () => {
  useWalletScenario({ connected: false })
  return (
    <StoryFrame>
      <WithdrawForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const ConnectedEmptyDarkPool = () => {
  useWalletScenario({ connected: true })
  return (
    <StoryFrame>
      <WithdrawForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const ConnectedWithUSDC = () => {
  useWalletScenario({ connected: true, internal: { usdc: '500' } })
  return (
    <StoryFrame>
      <WithdrawForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const ConnectedWithWETH = () => {
  // Seeded wallet starts with 1 WETH, so we can deposit 0.5 to leave
  // both wallet and DarkPool with non-zero balances.
  useWalletScenario({ connected: true, internal: { weth: '0.5' } })
  return (
    <StoryFrame>
      <WithdrawForm initialToken="WETH" />
    </StoryFrame>
  )
}

export const Paused = () => {
  useWalletScenario({ connected: true, paused: true, internal: { usdc: '500' } })
  return (
    <StoryFrame>
      <WithdrawForm initialToken="USDC" />
    </StoryFrame>
  )
}
