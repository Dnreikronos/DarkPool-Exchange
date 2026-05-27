import { afterEach, describe, expect, it, vi } from 'vitest'
import { arbitrum, foundry, mainnet, sepolia } from 'wagmi/chains'

// `wagmi-config` reads `../config` at module load to resolve
// `targetChain`. Mock it with a stable shape so the test file can
// import wagmi-config without requiring the full NEXT_PUBLIC_*
// environment.
vi.mock('../config', () => ({
  config: {
    useMocks: true,
    apiUrl: 'http://localhost',
    apiKey: 'test',
    chainId: 31337,
    operatorPubkeyUrl: 'http://localhost/pubkey',
    contracts: null,
  },
}))

import { resolveTargetChain, resolveWalletConnectProjectId } from './wagmi-config'

describe('resolveTargetChain', () => {
  it('returns the matching chain for a known id', () => {
    expect(resolveTargetChain(1).name).toBe(mainnet.name)
    expect(resolveTargetChain(31337).name).toBe(foundry.name)
  })

  it('throws with a helpful list when the id is unknown', () => {
    expect(() => resolveTargetChain(9999)).toThrow(/Unsupported NEXT_PUBLIC_CHAIN_ID=9999/)
  })
})

describe('resolveWalletConnectProjectId', () => {
  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it('returns a real id when one is configured (mainnet)', () => {
    vi.stubEnv('NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID', 'real-id-abc123')
    expect(resolveWalletConnectProjectId(mainnet)).toBe('real-id-abc123')
  })

  it('returns a real id when one is configured (testnet)', () => {
    vi.stubEnv('NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID', 'real-id-abc123')
    expect(resolveWalletConnectProjectId(sepolia)).toBe('real-id-abc123')
  })

  it('throws when the env var is missing and the chain is not a testnet', () => {
    vi.stubEnv('NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID', '')
    expect(() => resolveWalletConnectProjectId(mainnet)).toThrow(/required/)
    expect(() => resolveWalletConnectProjectId(arbitrum)).toThrow(/required/)
  })

  it('throws when the env var is the placeholder and the chain is not a testnet', () => {
    vi.stubEnv('NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID', 'YOUR_PROJECT_ID')
    expect(() => resolveWalletConnectProjectId(mainnet)).toThrow(/required/)
  })

  it('accepts the placeholder on testnet chains so dev workflows are unblocked', () => {
    vi.stubEnv('NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID', '')
    expect(resolveWalletConnectProjectId(sepolia)).toBe('YOUR_PROJECT_ID')
    expect(resolveWalletConnectProjectId(foundry)).toBe('YOUR_PROJECT_ID')
  })
})
