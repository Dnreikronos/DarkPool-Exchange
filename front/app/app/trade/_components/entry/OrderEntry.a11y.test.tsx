// @vitest-environment jsdom

// Automated axe-core scan of the order-entry panel (#80) in its
// disconnected and connected states. See front/test/axe.ts for scope.

import * as React from 'react'
import { afterEach, describe, it, vi } from 'vitest'
import { cleanup, render } from '@testing-library/react'

import { walletStore } from '@/lib/wallet/mock-store'
import { expectNoAxeViolations } from '@/test/axe'

// useRealSubmission requires DarkPoolClientProvider + a Web Worker. Tests
// run with NEXT_PUBLIC_USE_MOCKS='true' so the real path is never taken —
// stub it out (same as OrderEntry.test.tsx).
vi.mock('../../_hooks/entry/useRealSubmission', () => ({
  useRealSubmission: () => ({
    buildSteps: () => [],
    provingPct: null,
  }),
}))

import { OrderEntry } from './OrderEntry'

describe('OrderEntry a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    render(<OrderEntry />)
    await expectNoAxeViolations()
  })

  it('has no axe violations when connected', async () => {
    walletStore.connect()
    render(<OrderEntry />)
    await expectNoAxeViolations()
  })
})
