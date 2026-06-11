import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { MyOrdersEmpty, MyOrdersError, MyOrdersLoading } from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('my-orders state primitives', () => {
  it('Loading renders a 5-column skeleton matching the row layout', () => {
    const html = render(<MyOrdersLoading rows={3} />)
    const spans = html.match(/<span/g) ?? []
    // 3 rows * 5 cols = 15 + 1 sr-only span = 16
    expect(spans.length).toBe(16)
  })

  it('Empty surfaces the no-orders label by default', () => {
    const html = render(<MyOrdersEmpty />)
    expect(html).toContain('[ NO ORDERS YET ]')
  })

  it('Empty surfaces the connect-wallet label in disconnected mode', () => {
    const html = render(<MyOrdersEmpty disconnected />)
    expect(html).toContain('[ CONNECT WALLET ]')
  })

  it('Error surfaces the unavailable label + retry button', () => {
    const html = render(<MyOrdersError message="stream dropped" onRetry={vi.fn()} />)
    expect(html).toContain('[ ORDERS UNAVAILABLE ]')
    expect(html).toContain('stream dropped')
    expect(html).toContain('[ RETRY ]')
  })
})
