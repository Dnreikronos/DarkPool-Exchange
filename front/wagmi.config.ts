import { readFileSync } from 'node:fs'
import { defineConfig } from '@wagmi/cli'
import type { Abi } from 'viem'

// Source-of-truth ABIs committed under crates/dp-settlement/abi. Read at
// config-eval time so the generated file always mirrors them exactly —
// the drift test (generated.test.ts) fails CI if they diverge.
function loadAbi(path: string): Abi {
  const raw = JSON.parse(readFileSync(path, 'utf8'))
  // Guard the cast: a truncated file or a Foundry `{ "abi": [...] }`
  // wrapper would otherwise silently generate an incomplete output.
  if (!Array.isArray(raw)) {
    throw new Error(`${path}: expected a bare ABI JSON array`)
  }
  return raw as Abi
}

export default defineConfig({
  out: 'lib/contracts/generated.ts',
  contracts: [
    { name: 'darkPool', abi: loadAbi('../crates/dp-settlement/abi/DarkPool.json') },
    { name: 'verifierProxy', abi: loadAbi('../crates/dp-settlement/abi/VerifierProxy.json') },
  ],
})
