// SSR-contract tests for useTraderFills, in the repo's
// renderToStaticMarkup style (no DOM): the initial commit must be safe on
// the server (no IndexedDB touch) and render an empty fill list. The
// post-effect live behavior is covered by the repo/liveQuery layers
// (repo.test.ts) — this hook is subscription glue.

import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { useTraderFills } from './hooks'

const TRADER = 'aabbccddeeff00112233445566778899aabbccdd'

function Probe({ trader }: { trader: string | null }): JSX.Element {
  const fills = useTraderFills(trader)
  return <span data-count={fills.length}>{fills.length}</span>
}

describe('useTraderFills (SSR contract)', () => {
  it('renders an empty fill list on the server without touching IndexedDB', () => {
    // No indexedDB global exists in this node environment — any touch
    // during the render commit would throw.
    expect(renderToStaticMarkup(<Probe trader={TRADER} />)).toContain('>0<')
  })

  it('renders empty for a disconnected trader', () => {
    expect(renderToStaticMarkup(<Probe trader={null} />)).toContain('>0<')
  })
})
