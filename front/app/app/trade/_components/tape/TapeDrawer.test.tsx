// @vitest-environment jsdom
import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { create } from '@bufbuild/protobuf'
import { AuctionSummarySchema } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { TapeDrawer } from './TapeDrawer'

const TX = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

function auction() {
  return create(AuctionSummarySchema, {
    auctionId: 'a1b2c3d4e5f607',
    pair: 'ETH/USDC',
    clearingPrice: '3000.12',
    matchedVolume: '12.5',
    matchCount: 4,
    timestampUnix: 1700000000n,
  })
}

describe('TapeDrawer settlement linkage', () => {
  // Radix portals outlive each render without the RTL auto-cleanup
  // (vitest globals are off), so unmount explicitly.
  afterEach(cleanup)

  it('renders the Etherscan link for a settled auction', () => {
    render(
      <TapeDrawer
        auction={auction()}
        link={{ txHash: TX, url: `https://arbiscan.io/tx/${TX}` }}
        onClose={() => {}}
      />
    )
    const anchor = screen.getByRole('link', { name: /settlement transaction/i })
    expect(anchor).toHaveProperty('href', `https://arbiscan.io/tx/${TX}`)
    expect(anchor.textContent).toContain('0xdead…beef')
  })

  it('falls back to a plain hash when the chain has no explorer', () => {
    render(<TapeDrawer auction={auction()} link={{ txHash: TX, url: null }} onClose={() => {}} />)
    expect(screen.queryByRole('link', { name: /settlement transaction/i })).toBeNull()
    expect(screen.getByText(/0xdead…beef/)).toBeTruthy()
  })

  it('keeps the pending placeholder when no settlement is linked yet', () => {
    render(<TapeDrawer auction={auction()} link={null} onClose={() => {}} />)
    expect(screen.getByText('[ ETHERSCAN · PENDING ]')).toBeTruthy()
    expect(screen.queryByRole('link', { name: /settlement transaction/i })).toBeNull()
  })
})
