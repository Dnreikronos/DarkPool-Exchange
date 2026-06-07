// @vitest-environment jsdom

// Automated axe-core scan of the deposit/withdraw modals (#80). Radix
// dialogs portal into document.body, so the scan targets the whole
// document. Also locks the amount-validation error as role="alert".

import * as React from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

// Pin mock mode and stub wagmi inert (same as DepositModal.test.tsx) so
// the form stays on the walletStore path.
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
import { expectNoAxeViolations } from '@/test/axe'

import { DepositModal } from './DepositModal'
import { WithdrawModal } from './WithdrawModal'

describe('Deposit/Withdraw modal a11y', () => {
  beforeEach(() => {
    walletStore.connect()
    walletStore.resetTxState()
  })

  afterEach(() => {
    walletStore.disconnect()
    walletStore.resetTxState()
    cleanup()
  })

  it('deposit modal open has no axe violations', async () => {
    render(<DepositModal open onOpenChange={() => {}} />)
    await expectNoAxeViolations()
  })

  it('withdraw modal open has no axe violations', async () => {
    render(<WithdrawModal open onOpenChange={() => {}} />)
    await expectNoAxeViolations()
  })

  it('announces the amount validation error as an alert', async () => {
    render(<DepositModal open onOpenChange={() => {}} />)
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '-5' } })
    const alerts = screen.getAllByRole('alert')
    expect(alerts.some((a) => a.id === 'deposit-error')).toBe(true)
    await expectNoAxeViolations()
  })
})
