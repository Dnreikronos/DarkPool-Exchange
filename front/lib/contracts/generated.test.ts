import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, it, expect } from 'vitest'

import { darkPoolAbi, verifierProxyAbi } from './generated'

// front/lib/contracts -> ../../../ = repo root, then crates/dp-settlement/abi.
function loadSourceAbi(contract: string): unknown {
  const url = new URL(`../../../crates/dp-settlement/abi/${contract}.json`, import.meta.url)
  return JSON.parse(readFileSync(fileURLToPath(url), 'utf8'))
}

const CASES = [
  ['DarkPool', darkPoolAbi],
  ['VerifierProxy', verifierProxyAbi],
] as const

describe('contract ABI drift', () => {
  for (const [contract, generated] of CASES) {
    it(`${contract} generated ABI matches the committed JSON (run \`npm run codegen\` if this fails)`, () => {
      const source = loadSourceAbi(contract)
      // Strip the `as const` literal types down to plain JSON for the compare.
      expect(JSON.parse(JSON.stringify(generated))).toEqual(source)
    })
  }
})
