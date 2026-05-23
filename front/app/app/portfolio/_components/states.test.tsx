import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import {
  FillHistoryEmpty,
  FillHistoryError,
  FillHistoryLoading,
  PortfolioDisconnected,
  PortfolioError,
  PortfolioStatsLoading,
} from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('portfolio state primitives', () => {
  it('Disconnected surfaces the wallet prompt', () => {
    const html = render(<PortfolioDisconnected />)
    // React encodes `&` to `&amp;` in SSR markup.
    expect(html).toContain('[ CONNECT WALLET TO SEE POSITION + P&amp;L ]')
  })

  it('PortfolioStatsLoading renders one row of three stat skeletons', () => {
    const html = render(<PortfolioStatsLoading />)
    const spans = html.match(/<span/g) ?? []
    // 1 row * 3 cols = 3 + 1 sr-only span = 4
    expect(spans.length).toBe(4)
  })

  it('FillHistoryLoading renders rows of five-column skeletons', () => {
    const html = render(<FillHistoryLoading rows={2} />)
    const spans = html.match(/<span/g) ?? []
    // 2 rows * 5 cols = 10 + 1 sr-only span = 11
    expect(spans.length).toBe(11)
  })

  it('FillHistoryEmpty surfaces the terse no-fills copy with a hint', () => {
    const html = render(<FillHistoryEmpty />)
    expect(html).toContain('[ NO FILLS YET ]')
    expect(html).toContain('Place an order on /app/trade')
  })

  it('PortfolioError surfaces the unavailable label with a retry handler', () => {
    const html = render(<PortfolioError message="sync failed" onRetry={vi.fn()} />)
    expect(html).toContain('[ PORTFOLIO UNAVAILABLE ]')
    expect(html).toContain('sync failed')
    expect(html).toContain('[ RETRY ]')
  })

  it('FillHistoryError surfaces the fill-history label', () => {
    expect(render(<FillHistoryError />)).toContain('[ FILL HISTORY UNAVAILABLE ]')
  })
})
