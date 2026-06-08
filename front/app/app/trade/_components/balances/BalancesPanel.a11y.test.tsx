// @vitest-environment jsdom

// Automated axe-core scan of the balances panel (#80) in its
// disconnected and connected states. See front/test/axe.ts for scope.

import * as React from 'react'
import { afterEach, describe, it, vi } from 'vitest'
import { cleanup, render } from '@testing-library/react'

// Same inert stubs as DepositModal.test.tsx — useBalances pulls config +
// wagmi into the import graph.
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

import { BalancesPanel } from './BalancesPanel'

describe('BalancesPanel a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    render(<BalancesPanel />)
    await expectNoAxeViolations()
  })

  it('has no axe violations when connected', async () => {
    walletStore.connect()
    render(<BalancesPanel />)
    await expectNoAxeViolations()
  })
})
