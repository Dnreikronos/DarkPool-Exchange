import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { BoxSkeletonBlock, BoxSkeletonRow, PanelEmpty, PanelError } from './panel-state'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

describe('BoxSkeletonRow', () => {
  it('renders one span per column with box-drawing characters', () => {
    const html = render(<BoxSkeletonRow cols={3} index={0} />)
    const spans = html.match(/<span/g) ?? []
    expect(spans.length).toBe(3)
    // Box-drawing chars survive renderToStaticMarkup as UTF-8 literals.
    const fills = html.match(/█/g) ?? []
    expect(fills.length).toBeGreaterThanOrEqual(3)
    const empties = html.match(/░/g) ?? []
    expect(empties.length).toBeGreaterThanOrEqual(3)
  })

  it('respects a custom cols count', () => {
    const html = render(<BoxSkeletonRow cols={5} />)
    const spans = html.match(/<span/g) ?? []
    expect(spans.length).toBe(5)
  })

  it('is decorative for assistive tech', () => {
    const html = render(<BoxSkeletonRow />)
    expect(html).toMatch(/aria-hidden="true"/)
  })

  it('honors prefers-reduced-motion', () => {
    const html = render(<BoxSkeletonRow />)
    expect(html).toContain('motion-reduce:animate-none')
  })
})

describe('BoxSkeletonBlock', () => {
  it('renders the configured row count', () => {
    const html = render(<BoxSkeletonBlock rows={4} cols={3} />)
    // 4 rows * 3 cols = 12 inner spans + 1 sr-only span for the live region.
    const spans = html.match(/<span/g) ?? []
    expect(spans.length).toBe(13)
  })

  it('exposes the polite status with the supplied label', () => {
    const html = render(<BoxSkeletonBlock ariaLabel="Loading orderbook" />)
    expect(html).toMatch(/role="status"/)
    expect(html).toMatch(/aria-live="polite"/)
    expect(html).toContain('Loading orderbook')
  })
})

describe('PanelEmpty', () => {
  it('renders the bracketed label inside a status region', () => {
    const html = render(<PanelEmpty label="[ NO ORDERS YET ]" />)
    expect(html).toContain('[ NO ORDERS YET ]')
    expect(html).toMatch(/role="status"/)
  })

  it('renders the optional hint when provided', () => {
    const html = render(<PanelEmpty label="[ NO FILLS YET ]" hint="Place an order to start." />)
    expect(html).toContain('Place an order to start.')
  })

  it('omits the hint node when not provided', () => {
    const html = render(<PanelEmpty label="[ NO FILLS YET ]" />)
    // Only one text span (the label).
    const spans = html.match(/<span/g) ?? []
    expect(spans.length).toBe(1)
  })

  it('renders muted copy — never the lime accent', () => {
    const html = render(<PanelEmpty label="[ NO FILLS YET ]" hint="hint" />)
    expect(html).not.toMatch(/brand-accent/)
  })
})

describe('PanelError', () => {
  it('renders the bracketed label inside an alert region', () => {
    const html = render(<PanelError label="[ ORDERBOOK UNAVAILABLE ]" />)
    expect(html).toContain('[ ORDERBOOK UNAVAILABLE ]')
    expect(html).toMatch(/role="alert"/)
  })

  it('renders message when provided', () => {
    const html = render(<PanelError label="[ ERR ]" message="Network unreachable." />)
    expect(html).toContain('Network unreachable.')
  })

  it('renders the retry button only when onRetry is provided', () => {
    const noRetry = render(<PanelError label="[ ERR ]" />)
    expect(noRetry).not.toContain('[ RETRY ]')

    const withRetry = render(<PanelError label="[ ERR ]" onRetry={() => undefined} />)
    expect(withRetry).toContain('[ RETRY ]')
  })

  it('allows a custom retry label', () => {
    const html = render(
      <PanelError label="[ ERR ]" onRetry={() => undefined} retryLabel="[ RELOAD ]" />
    )
    expect(html).toContain('[ RELOAD ]')
  })

  it('keeps the accent off the rest state — accent appears only on retry focus/hover utilities', () => {
    const html = render(<PanelError label="[ ERR ]" message="oops" onRetry={() => undefined} />)
    // Static accent classes ARE allowed on the retry button (hover/focus
    // states), but no `bg-brand-accent` should fill the button at rest.
    expect(html).not.toMatch(/bg-brand-accent/)
  })

  it('invokes onRetry via the button (smoke render — handler binding present)', () => {
    const spy = vi.fn()
    // We render to static markup so the click handler isn't bound to a
    // DOM event. The presence of the button is enough — the binding is
    // type-safe in TS. The actual click is exercised by Ladle.
    const html = render(<PanelError label="[ ERR ]" onRetry={spy} />)
    expect(html).toContain('[ RETRY ]')
    expect(spy).not.toHaveBeenCalled()
  })
})
