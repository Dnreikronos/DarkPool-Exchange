import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// config.ts parses process.env at module-load, so each case sets env then
// re-imports a fresh module graph.
const BASE_ENV: Record<string, string> = {
  NEXT_PUBLIC_USE_MOCKS: 'true',
  NEXT_PUBLIC_DARKPOOL_API_URL: 'http://localhost:8080',
  NEXT_PUBLIC_DARKPOOL_API_KEY: 'dev-key',
  NEXT_PUBLIC_CHAIN_ID: '31337',
  NEXT_PUBLIC_OPERATOR_PUBKEY_URL: 'http://localhost:8080/v1/operator/pubkey',
}

const ORIGINAL = { ...process.env }

beforeEach(() => {
  vi.resetModules()
  for (const key of Object.keys(process.env)) {
    if (key.startsWith('NEXT_PUBLIC_')) delete process.env[key]
  }
  Object.assign(process.env, BASE_ENV)
})

afterEach(() => {
  for (const key of Object.keys(process.env)) {
    if (!(key in ORIGINAL)) delete process.env[key]
  }
  Object.assign(process.env, ORIGINAL)
})

describe('config.siweEnabled (NEXT_PUBLIC_SIWE_ENABLED)', () => {
  it('defaults to false when unset, without throwing at load', async () => {
    delete process.env.NEXT_PUBLIC_SIWE_ENABLED
    const { config } = await import('./config')
    expect(config.siweEnabled).toBe(false)
  })

  it('is true when set to "true"', async () => {
    process.env.NEXT_PUBLIC_SIWE_ENABLED = 'true'
    const { config } = await import('./config')
    expect(config.siweEnabled).toBe(true)
  })

  it('is true when set to "1"', async () => {
    process.env.NEXT_PUBLIC_SIWE_ENABLED = '1'
    const { config } = await import('./config')
    expect(config.siweEnabled).toBe(true)
  })

  it('is false when set to "false"', async () => {
    process.env.NEXT_PUBLIC_SIWE_ENABLED = 'false'
    const { config } = await import('./config')
    expect(config.siweEnabled).toBe(false)
  })
})
