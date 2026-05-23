import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { BalancesDisconnected, BalancesError, BalancesLoading } from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('balances state primitives', () => {
  it('Loading renders one skeleton row per token (default 2 rows)', () => {
    const html = render(<BalancesLoading />)
    const spans = html.match(/<span/g) ?? []
    // 2 rows * 3 cols = 6 + 1 sr-only span = 7
    expect(spans.length).toBe(7)
  })

  it('Disconnected surfaces the connect-wallet label', () => {
    const html = render(<BalancesDisconnected />)
    expect(html).toContain('[ CONNECT WALLET ]')
  })

  it('Error surfaces the unavailable label + retry button', () => {
    const html = render(<BalancesError message="rpc down" onRetry={vi.fn()} />)
    expect(html).toContain('[ BALANCES UNAVAILABLE ]')
    expect(html).toContain('rpc down')
    expect(html).toContain('[ RETRY ]')
  })
})
