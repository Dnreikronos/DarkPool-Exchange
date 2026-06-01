import { createSiweMessage } from 'viem/siwe'
import type { Address } from './types'

/**
 * Builds the canonical EIP-4361 (SIWE) message string the wallet signs.
 *
 * Uses viem's `createSiweMessage` (viem ships native SIWE support, so no
 * extra `siwe` dependency). The `nonce` MUST be the server-issued one
 * from `GET /v1/auth/nonce` — never a client-generated nonce, or the
 * backend's single-use nonce store rejects the signature.
 *
 * `domain`/`uri` default to the current page authority/origin so they
 * match what the backend validates when `DARKPOOL_SIWE_DOMAIN` is set.
 */
export const SIWE_STATEMENT =
  'Sign in to DarkPool. This request will not trigger a blockchain transaction or cost gas.'

export interface BuildSiweMessageParams {
  /** Checksummed wallet address from wagmi. */
  address: Address
  /** Connected chain id; must match `DARKPOOL_CHAIN_ID` when the server enforces it. */
  chainId: number
  /** Server-issued single-use nonce. */
  nonce: string
  /** RFC-3986 authority (`host[:port]`, no scheme). Defaults to `window.location.host`. */
  domain?: string
  /** Defaults to `window.location.origin`. */
  uri?: string
  statement?: string
}

export function buildSiweMessage(params: BuildSiweMessageParams): string {
  const domain = params.domain ?? window.location.host
  const uri = params.uri ?? window.location.origin
  return createSiweMessage({
    address: params.address,
    chainId: params.chainId,
    domain,
    uri,
    nonce: params.nonce,
    version: '1',
    statement: params.statement ?? SIWE_STATEMENT,
  })
}
