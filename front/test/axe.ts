import * as axe from 'axe-core'

// WCAG 2.0/2.1 A + AA — the bar issue #80 sets for /trade and /portfolio.
// Best-practice-only rules (region, landmark-unique, …) are excluded on
// purpose: panels render as fragments in tests and would false-positive.
const WCAG_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']

/**
 * Run axe against a node and throw a readable report when violations exist.
 *
 * Defaults to the whole `document` so Radix portals (Dialog, Sheet, Toast)
 * mounted on `document.body` are included in the scan.
 *
 * `color-contrast` is disabled because jsdom has no layout engine, so axe
 * cannot resolve computed colors here. Contrast is covered by the manual
 * review tracked in issue #80 (DESIGN.md already regulates `brand-muted`).
 *
 * Note: axe-core errors if two runs overlap on one document — always
 * `await` each call before starting the next.
 */
export async function expectNoAxeViolations(node: Element | Document = document): Promise<void> {
  const results = await axe.run(node, {
    runOnly: { type: 'tag', values: WCAG_TAGS },
    rules: {
      'color-contrast': { enabled: false },
      // jsdom test documents have no <title>/lang — the real app sets both
      // via the root layout (`<html lang="en">` + Next metadata). Pure
      // environment artifacts, not app violations.
      'document-title': { enabled: false },
      'html-has-lang': { enabled: false },
    },
  })
  if (results.violations.length === 0) return
  const report = results.violations
    .map((v) =>
      [
        `${v.id} (${v.impact ?? 'unknown'}): ${v.help}`,
        ...v.nodes.map((n) => `  ${n.html}\n  ${n.failureSummary ?? ''}`),
      ].join('\n')
    )
    .join('\n\n')
  throw new Error(`axe violations:\n\n${report}`)
}
