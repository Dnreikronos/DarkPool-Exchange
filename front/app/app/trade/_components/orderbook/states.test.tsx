import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { OrderBookEmpty, OrderBookError, OrderBookLoading } from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('orderbook state primitives', () => {
  it('Loading renders an a11y-tagged box-drawing skeleton', () => {
    const html = render(<OrderBookLoading rows={5} />)
    expect(html).toMatch(/role="status"/)
    expect(html).toMatch(/aria-label="Loading orderbook"/)
    // 5 rows * 3 cols = 15 + 1 sr-only span = 16
    const spans = html.match(/<span/g) ?? []
    expect(spans.length).toBe(16)
  })

  it('Empty renders the bracketed label inside a status region', () => {
    const html = render(<OrderBookEmpty />)
    expect(html).toContain('[ NO ORDERS YET ]')
    expect(html).toMatch(/role="status"/)
  })

  it('Error renders the bracketed label and message', () => {
    const html = render(<OrderBookError message="Network refused" />)
    expect(html).toContain('[ ORDERBOOK UNAVAILABLE ]')
    expect(html).toContain('Network refused')
    expect(html).toMatch(/role="alert"/)
  })

  it('Error renders the retry button only when onRetry is provided', () => {
    const noRetry = render(<OrderBookError />)
    expect(noRetry).not.toContain('[ RETRY ]')

    const withRetry = render(<OrderBookError onRetry={vi.fn()} />)
    expect(withRetry).toContain('[ RETRY ]')
  })
})
