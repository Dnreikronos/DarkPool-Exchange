// @vitest-environment jsdom

// Automated axe-core scan of the /portfolio surface (#80). Renders the
// real panel in jsdom (fake-indexeddb backs the Dexie fill history) and
// asserts zero WCAG A/AA violations — see front/test/axe.ts for scope.

import 'fake-indexeddb/auto'

import * as React from 'react'
import { afterEach, describe, it } from 'vitest'
import { cleanup, render } from '@testing-library/react'

import type { Fill } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '@/lib/wallet/mock-store'
import { expectNoAxeViolations } from '@/test/axe'

import { FillHistoryTable } from './FillHistoryTable'
import { PortfolioPanel } from './PortfolioPanel'

function fill(overrides: Partial<Fill> = {}): Fill {
  return {
    fillId: 'f-001',
    orderId: 'o-001',
    auctionId: 'a-001',
    side: Side.BUY,
    price: '3000.12',
    size: '1.5',
    timestampUnix: 1_700_000_000n,
    ...overrides,
  }
}

describe('PortfolioPanel a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    render(<PortfolioPanel />)
    await expectNoAxeViolations()
  })

  it('has no axe violations when connected', async () => {
    walletStore.connect()
    render(<PortfolioPanel />)
    await expectNoAxeViolations()
  })

  it('has no axe violations with a populated fill history', async () => {
    render(
      <FillHistoryTable
        fills={[fill(), fill({ fillId: 'f-002', orderId: 'o-002', side: Side.SELL })]}
      />
    )
    await expectNoAxeViolations()
  })
})
