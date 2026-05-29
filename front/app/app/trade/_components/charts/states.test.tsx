import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { ChartError, ChartLoading, DepthChartEmpty, PriceChartEmpty } from './states'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('charts state primitives', () => {
  it('ChartLoading renders a polite live region with a shimmer block', () => {
    const html = render(<ChartLoading />)
    expect(html).toMatch(/role="status"/)
    expect(html).toMatch(/aria-live="polite"/)
    // Reduced-motion respect comes from the shared Skeleton primitive.
    expect(html).toContain('motion-reduce:animate-none')
  })

  it('DepthChartEmpty surfaces the no-book label', () => {
    expect(render(<DepthChartEmpty />)).toContain('[ NO BOOK YET ]')
  })

  it('PriceChartEmpty surfaces the no-history label', () => {
    expect(render(<PriceChartEmpty />)).toContain('[ NO PRICE HISTORY ]')
  })

  it('ChartError surfaces the default label and supports a retry handler', () => {
    const html = render(<ChartError message="render failed" onRetry={vi.fn()} />)
    expect(html).toContain('[ CHART UNAVAILABLE ]')
    expect(html).toContain('render failed')
    expect(html).toContain('[ RETRY ]')
  })

  it('ChartError accepts a custom label', () => {
    expect(render(<ChartError label="[ DEPTH UNAVAILABLE ]" />)).toContain('[ DEPTH UNAVAILABLE ]')
  })
})
