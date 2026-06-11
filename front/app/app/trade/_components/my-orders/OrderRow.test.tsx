// @vitest-environment jsdom
import * as React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { create } from '@bufbuild/protobuf'
import { OrderInfoSchema, Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { MyOrderRow } from '../../_lib/my-orders/types'
import { OrderRow } from './OrderRow'

const TX = '0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef'

function row(status: MyOrderRow['status']): MyOrderRow {
  return {
    status,
    order: create(OrderInfoSchema, {
      id: 'order-1',
      pair: 'ETH/USDC',
      side: Side.BUY,
      price: '3000.12',
      size: '1.5',
      remainingSize: '0',
      submittedAtUnix: 1700000000n,
      expiresAtUnix: 1700000600n,
    }),
  }
}

describe('OrderRow settlement linkage', () => {
  afterEach(cleanup)

  it('shows the settlement tx link in place of the cancel action on filled rows', () => {
    render(
      <OrderRow
        row={row('filled')}
        link={{ txHash: TX, url: `https://arbiscan.io/tx/${TX}` }}
        onCancel={vi.fn()}
      />
    )
    const anchor = screen.getByRole('link', { name: /settlement transaction/i })
    expect(anchor).toHaveProperty('href', `https://arbiscan.io/tx/${TX}`)
    expect(anchor.textContent).toContain('0xdead…beef')
    expect(screen.queryByRole('button', { name: /cancel order/i })).toBeNull()
  })

  it('renders the tx hash as plain text on explorerless chains', () => {
    render(<OrderRow row={row('filled')} link={{ txHash: TX, url: null }} onCancel={vi.fn()} />)
    expect(screen.queryByRole('link')).toBeNull()
    expect(screen.getByText(/0xdead…beef/)).toBeTruthy()
  })

  it('keeps the cancel affordance on open rows even if a link is passed', () => {
    render(
      <OrderRow
        row={row('open')}
        link={{ txHash: TX, url: `https://arbiscan.io/tx/${TX}` }}
        onCancel={vi.fn()}
      />
    )
    expect(screen.getByRole('button', { name: /cancel order/i })).toBeTruthy()
    expect(screen.queryByRole('link')).toBeNull()
  })

  it('keeps the disabled cancel button on filled rows without a link', () => {
    render(<OrderRow row={row('filled')} onCancel={vi.fn()} />)
    const button = screen.getByRole('button', { name: /cancel order/i })
    expect(button).toHaveProperty('disabled', true)
  })
})
