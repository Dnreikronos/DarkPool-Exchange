import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { TapeEmpty, TapeError, TapeLoading } from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('tape state primitives', () => {
  it('Loading announces and renders a 4-column skeleton', () => {
    const html = render(<TapeLoading rows={3} />)
    expect(html).toMatch(/aria-label="Loading auction tape"/)
    const spans = html.match(/<span/g) ?? []
    // 3 rows * 4 cols = 12 + 1 sr-only span = 13
    expect(spans.length).toBe(13)
  })

  it('Empty surfaces the no-auctions label', () => {
    const html = render(<TapeEmpty />)
    expect(html).toContain('[ NO AUCTIONS YET ]')
  })

  it('Error surfaces the unavailable label with a retry handler', () => {
    const html = render(<TapeError message="timeout" onRetry={vi.fn()} />)
    expect(html).toContain('[ TAPE UNAVAILABLE ]')
    expect(html).toContain('timeout')
    expect(html).toContain('[ RETRY ]')
  })
})
