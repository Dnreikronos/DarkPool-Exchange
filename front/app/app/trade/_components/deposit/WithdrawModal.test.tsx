// Smoke tests for the withdraw form composition. Same factoring
// rationale as `DepositModal.test.tsx` — we render the form body
// rather than the modal wrapper because Radix Portal emits no SSR.

import * as React from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

// See DepositModal.test.tsx — pin mock mode + inert wagmi so the wired
// form renders on the walletStore path under node SSR.
vi.mock('@/lib/config', () => ({
  config: { useMocks: true, chainId: 31337, contracts: null },
}))
vi.mock('wagmi', () => ({
  useReadContracts: () => ({
    data: undefined,
    isLoading: false,
    isError: false,
    refetch: () => {},
  }),
  useWatchContractEvent: () => {},
  useWriteContract: () => ({ writeContractAsync: async () => '0x' }),
  useConfig: () => ({}),
}))
vi.mock('wagmi/actions', () => ({ waitForTransactionReceipt: async () => ({}) }))

import { walletStore } from '@/lib/wallet/mock-store'

import { WithdrawForm } from './WithdrawForm'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('WithdrawForm', () => {
  beforeEach(() => {
    walletStore.disconnect()
    walletStore.resetTxState()
  })

  afterEach(() => {
    walletStore.disconnect()
    walletStore.resetTxState()
  })

  it('renders the bracketed header, token toggle, and DarkPool balance', () => {
    walletStore.connect()
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('WITHDRAW')
    expect(html).toContain('USDC')
    expect(html).toContain('[ TOKEN ]')
    expect(html).toContain('[ AMOUNT ]')
    expect(html).toContain('[ DARKPOOL BALANCE ]')
    expect(html).toContain('01')
    expect(html).toContain('Withdraw')
  })

  it('shows the paused notice + PAUSED primary label when tx state is paused', () => {
    walletStore.connect()
    walletStore.setPaused(true)
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('[ CONTRACT PAUSED ]')
    expect(html).toMatch(/Withdrawals are suspended/i)
    expect(html).toContain('PAUSED')
  })

  it('surfaces CONNECT WALLET when disconnected', () => {
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('CONNECT WALLET')
  })

  it('reflects the seeded DarkPool balance (0 by default after connect)', () => {
    walletStore.connect()
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('0.00')
  })

  it('reflects a non-zero internal balance after a deposit roundtrip', () => {
    walletStore.connect()
    walletStore.approve('USDC', '500')
    walletStore.deposit('USDC', '500')
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('500.00')
  })

  it('exposes the dev simulate-revert affordance in test/dev builds', () => {
    walletStore.connect()
    const html = render(<WithdrawForm initialToken="USDC" />)
    expect(html).toContain('[ DEV · SIMULATE REVERT ]')
    expect(html).toContain('[ ON WITHDRAW ]')
  })
})
