import * as React from 'react'

import { walletStore } from '../../../lib/wallet/mock-store'

import { DepositForm } from './DepositForm'

// Stories render the form body in isolation. Modals are exercised in
// the dedicated DepositModal / WithdrawModal / DepositTriggers stories
// so reviewers can scrub through every interaction state without
// the modal chrome stealing focus.

function useWalletScenario({
  connected,
  paused,
  allowance,
  internal,
}: {
  connected: boolean
  paused?: boolean
  allowance?: { weth?: string; usdc?: string }
  internal?: { weth?: string; usdc?: string }
}) {
  React.useEffect(() => {
    walletStore.resetTxState()
    if (connected) walletStore.connect()
    else walletStore.disconnect()
    if (allowance?.weth) walletStore.approve('WETH', allowance.weth)
    if (allowance?.usdc) walletStore.approve('USDC', allowance.usdc)
    // Seed a fake internal balance through the public mutators for the
    // "ready to withdraw" stories — we deposit and immediately drain
    // the allowance so the user sees a non-zero darkpool balance.
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
  }, [connected, paused, allowance?.weth, allowance?.usdc, internal?.weth, internal?.usdc])
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
      <DepositForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const ConnectedNoAllowance = () => {
  useWalletScenario({ connected: true })
  return (
    <StoryFrame>
      <DepositForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const ConnectedWithAllowance = () => {
  useWalletScenario({ connected: true, allowance: { usdc: '1000' } })
  return (
    <StoryFrame>
      <DepositForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const Paused = () => {
  useWalletScenario({ connected: true, paused: true })
  return (
    <StoryFrame>
      <DepositForm initialToken="USDC" />
    </StoryFrame>
  )
}

export const WETHPath = () => {
  useWalletScenario({ connected: true })
  return (
    <StoryFrame>
      <DepositForm initialToken="WETH" />
    </StoryFrame>
  )
}
