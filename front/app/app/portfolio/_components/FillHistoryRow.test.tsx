// @vitest-environment jsdom
import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import type { Fill } from '@/lib/mock-store'

import { FillHistoryRow } from './FillHistoryRow'

const TX = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

function fill(): Fill {
  return {
    fillId: 'fill-1',
    orderId: 'order-1',
    auctionId: 'a1b2c3d4e5f607',
    side: Side.BUY,
    price: '3000.12',
    size: '1.5',
    timestampUnix: 1700000000n,
  }
}

describe('FillHistoryRow settlement linkage', () => {
  afterEach(cleanup)

  it('renders the settlement tx as an Etherscan link when correlated', () => {
    render(
      <ol>
        <FillHistoryRow fill={fill()} link={{ txHash: TX, url: `https://arbiscan.io/tx/${TX}` }} />
      </ol>
    )
    const anchor = screen.getByRole('link', { name: /settlement transaction/i })
    expect(anchor).toHaveProperty('href', `https://arbiscan.io/tx/${TX}`)
    expect(anchor.textContent).toContain('0xdead…beef')
  })

  it('renders the tx hash as plain text on explorerless chains', () => {
    render(
      <ol>
        <FillHistoryRow fill={fill()} link={{ txHash: TX, url: null }} />
      </ol>
    )
    expect(screen.queryByRole('link')).toBeNull()
    expect(screen.getByText(/0xdead…beef/)).toBeTruthy()
  })

  it('keeps the auction-id batch placeholder when not yet settled', () => {
    render(
      <ol>
        <FillHistoryRow fill={fill()} />
      </ol>
    )
    expect(screen.queryByRole('link')).toBeNull()
    expect(screen.getByText(/a1b2c3…f607/)).toBeTruthy()
  })
})
