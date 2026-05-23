// Render-shape tests for the onboarding panel. Radix `<Dialog>` portals
// into the DOM and emits no markup under `renderToStaticMarkup`, so we
// test the pure `<OnboardingPanel>` (which is also what `<Dialog>` ends
// up rendering inside the portal). Interactive behaviour (open/close,
// back/next chain) is exercised in the Ladle stories.

import * as React from 'react'
import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

import { OnboardingPanel } from './OnboardingDialog'
import {
  ONBOARDING_BACK_LABEL,
  ONBOARDING_DONE_LABEL,
  ONBOARDING_NEXT_LABEL,
  ONBOARDING_STEPS,
} from './copy'

function render(node: React.ReactElement): string {
  return renderToStaticMarkup(node)
}

function panel(step: number) {
  return <OnboardingPanel step={step} onBack={vi.fn()} onNext={vi.fn()} onDismiss={vi.fn()} />
}

describe('OnboardingPanel', () => {
  it('renders all three step indicators at every step', () => {
    for (const step of [0, 1, 2]) {
      const html = render(panel(step))
      expect(html).toContain('01')
      expect(html).toContain('02')
      expect(html).toContain('03')
    }
  })

  it('uses the lime accent only on the current step indicator', () => {
    const html = render(panel(1))
    // Each step renders inside a separate <li>. The current one carries
    // both `border-brand-accent` and `text-brand-accent`; the others
    // carry `border-brand-border` / `border-brand-border2` and muted
    // or fg text. Count accent hits — there should be exactly two
    // (border + text on the one current node).
    const accentHits = html.match(/brand-accent/g) ?? []
    expect(accentHits.length).toBe(2)
  })

  it('marks the current step with aria-current="step"', () => {
    const html = render(panel(1))
    const currentMatches = html.match(/aria-current="step"/g) ?? []
    expect(currentMatches.length).toBe(1)
  })

  it('renders the step kicker, title, and body for step 0', () => {
    const html = render(panel(0))
    expect(html).toContain(ONBOARDING_STEPS[0].kicker)
    expect(html).toContain(ONBOARDING_STEPS[0].title)
    expect(html).toContain(ONBOARDING_STEPS[0].body[0])
  })

  it('renders the step body for the middle step', () => {
    const html = render(panel(1))
    expect(html).toContain(ONBOARDING_STEPS[1].title)
  })

  it('shows the optional meta line when the step defines one', () => {
    const html = render(panel(0))
    expect(html).toContain('Batch cadence: 5 s.')
  })

  it('disables the back button on the first step', () => {
    const html = render(panel(0))
    // Locate the BACK button: its opening tag through the label text.
    const backStart = html.indexOf(ONBOARDING_BACK_LABEL)
    expect(backStart).toBeGreaterThan(-1)
    const before = html.slice(0, backStart)
    const lastButton = before.lastIndexOf('<button')
    expect(lastButton).toBeGreaterThan(-1)
    const buttonTag = before.slice(lastButton)
    expect(buttonTag).toMatch(/disabled=""/)
  })

  it('enables the back button on a later step', () => {
    const html = render(panel(1))
    const backStart = html.indexOf(ONBOARDING_BACK_LABEL)
    expect(backStart).toBeGreaterThan(-1)
    const before = html.slice(0, backStart)
    const lastButton = before.lastIndexOf('<button')
    expect(lastButton).toBeGreaterThan(-1)
    const buttonTag = before.slice(lastButton)
    expect(buttonTag).not.toMatch(/disabled=""/)
  })

  it('shows the NEXT label on intermediate steps', () => {
    const html = render(panel(0))
    expect(html).toContain(ONBOARDING_NEXT_LABEL)
    expect(html).not.toContain(ONBOARDING_DONE_LABEL)
  })

  it('promotes the primary CTA to START TRADING on the last step', () => {
    const html = render(panel(2))
    expect(html).toContain(ONBOARDING_DONE_LABEL)
    expect(html).not.toContain(ONBOARDING_NEXT_LABEL)
    // The lime CTA carries `bg-brand-accent`. This is the dialog's accent
    // budget when on the final step; the step-node accent has already
    // walked to step 03 at this point.
    expect(html).toContain('bg-brand-accent')
  })

  it('clamps step out-of-range to the nearest valid index', () => {
    const negative = render(
      <OnboardingPanel step={-3} onBack={vi.fn()} onNext={vi.fn()} onDismiss={vi.fn()} />
    )
    expect(negative).toContain(ONBOARDING_STEPS[0].title)

    const tooFar = render(
      <OnboardingPanel step={99} onBack={vi.fn()} onNext={vi.fn()} onDismiss={vi.fn()} />
    )
    expect(tooFar).toContain(ONBOARDING_STEPS[2].title)
  })
})
