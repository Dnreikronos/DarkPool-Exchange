// @vitest-environment jsdom

// Automated axe-core scan of the auction tape (#80): populated list with
// the LIVE badge, and the auction drawer open (Radix portal → scan the
// whole document). See front/test/axe.ts for the rule scope.

import { create } from '@bufbuild/protobuf'
import * as React from 'react'
import { afterEach, describe, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'

vi.mock('@/lib/config', () => ({
  config: { useMocks: true, contracts: null, chainId: 31337, siweEnabled: false },
}))

import type { DarkPoolClient } from '@/lib/sdk/client'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'
import {
  AuctionSummarySchema,
  GetAuctionHistoryResponseSchema,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { expectNoAxeViolations } from '@/test/axe'

import { Tape } from './Tape'
import { TapeDrawer } from './TapeDrawer'

const TX = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

function auction(id: string, ts: bigint) {
  return create(AuctionSummarySchema, {
    auctionId: id,
    pair: 'ETH/USDC',
    clearingPrice: '3000.12',
    matchedVolume: '12.5',
    matchCount: 4,
    timestampUnix: ts,
  })
}

function fakeClient(): DarkPoolClient {
  const history = create(GetAuctionHistoryResponseSchema, {
    auctions: [auction('a-2', 1_700_000_005n), auction('a-1', 1_700_000_000n)],
  })
  return {
    placeOrder: vi.fn(),
    cancelOrder: vi.fn(),
    getOrder: vi.fn(),
    getOrderBook: vi.fn(),
    getAuctionHistory: async () => history,
    streamAuctions: () =>
      (async function* () {
        await new Promise(() => {}) // hold the stream open → status "live"
      })(),
  } as unknown as DarkPoolClient
}

describe('Tape a11y', () => {
  afterEach(cleanup)

  it('has no axe violations with a populated tape', async () => {
    render(
      <DarkPoolClientProvider client={fakeClient()}>
        <Tape refetchIntervalMs={3_600_000} />
      </DarkPoolClientProvider>
    )
    await waitFor(() => {
      if (screen.queryAllByText(/3,?000\.12/).length === 0) {
        throw new Error('tape rows not loaded yet')
      }
    })
    await expectNoAxeViolations()
  })

  it('has no axe violations with the auction drawer open', async () => {
    render(
      <TapeDrawer
        auction={auction('a-1', 1_700_000_000n)}
        link={{ txHash: TX, url: `https://arbiscan.io/tx/${TX}` }}
        onClose={() => {}}
      />
    )
    await expectNoAxeViolations()
  })
})
