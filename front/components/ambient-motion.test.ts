// Ambient looping animations must freeze under `prefers-reduced-motion`
// (DESIGN.md motion contract). The trading app already pairs every loop
// with `motion-reduce:animate-none` (StatusPill, StreamStatus); the
// landing page shipped before that rule and animated unconditionally
// (#205 item 3).
//
// This is a source scan rather than a render assertion on purpose: jsdom
// has no layout engine and never evaluates the media query, so rendering
// cannot tell a frozen loop from a running one. Scanning also covers
// components no test renders today, and catches the next loop someone
// adds without the pairing.

import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

// Loops that run forever. One-shot transitions (fade-in, slide-in) are
// not ambient — they are allowed to keep their `animate-*` class.
const AMBIENT_ANIMATIONS = ['animate-marquee', 'animate-terminal-scroll', 'animate-blink']

const ROOTS = ['components', 'app']

function sourceFiles(dir: string): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name.startsWith('.')) continue
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      out.push(...sourceFiles(path))
    } else if (/\.tsx?$/.test(entry.name) && !/\.(test|stories)\.tsx?$/.test(entry.name)) {
      out.push(path)
    }
  }
  return out
}

describe('ambient motion respects prefers-reduced-motion', () => {
  it('pairs every ambient animation class with motion-reduce:animate-none', () => {
    const offenders: string[] = []

    for (const root of ROOTS) {
      for (const file of sourceFiles(root)) {
        const source = readFileSync(file, 'utf8')
        for (const animation of AMBIENT_ANIMATIONS) {
          if (!source.includes(animation)) continue
          // Check per class-string occurrence: a file may hold several,
          // and one paired usage must not vouch for an unpaired one.
          for (const line of source.split('\n')) {
            if (!line.includes(animation)) continue
            if (line.includes('motion-reduce:animate-none')) continue
            offenders.push(`${file}: ${line.trim()}`)
          }
        }
      }
    }

    expect(offenders).toEqual([])
  })
})
