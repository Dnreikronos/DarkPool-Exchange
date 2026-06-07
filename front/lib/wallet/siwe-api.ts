import type { Address } from './types'

/**
 * Thin client for the backend SIWE auth endpoints
 * (`GET /v1/auth/nonce`, `POST /v1/auth/verify`). These routes are
 * unauthenticated (they are how you obtain a session), so no auth header
 * is sent. `fetch` is injectable for tests.
 */

/** Error from a SIWE auth endpoint, carrying the HTTP status when known. */
export class SiweApiError extends Error {
  constructor(
    message: string,
    readonly status: number | null
  ) {
    super(message)
    this.name = 'SiweApiError'
  }
}

export interface VerifyResult {
  token: string
  /** Unix seconds — backend `expires_at`. */
  expiresAt: number
  address: Address
}

function normaliseBaseUrl(apiUrl: string): string {
  return apiUrl.replace(/\/+$/, '')
}

async function readMessage(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { message?: unknown }
    if (typeof body.message === 'string' && body.message.length > 0) return body.message
  } catch {
    // non-JSON error body
  }
  return `HTTP ${response.status}`
}

/** Fetch a fresh single-use nonce to embed in the SIWE message. */
export async function fetchNonce(apiUrl: string, fetchImpl: typeof fetch = fetch): Promise<string> {
  const url = `${normaliseBaseUrl(apiUrl)}/v1/auth/nonce`
  let response: Response
  try {
    response = await fetchImpl(url, { method: 'GET', headers: { accept: 'application/json' } })
  } catch (cause) {
    throw new SiweApiError(
      `network error fetching nonce: ${(cause as Error)?.message ?? cause}`,
      null
    )
  }
  if (!response.ok) {
    throw new SiweApiError(await readMessage(response), response.status)
  }
  const body = (await response.json()) as { nonce?: unknown }
  if (typeof body.nonce !== 'string' || body.nonce.length === 0) {
    throw new SiweApiError('nonce response missing `nonce`', response.status)
  }
  return body.nonce
}

/** Exchange a signed SIWE message for a session token. */
export async function verifySiwe(
  apiUrl: string,
  body: { message: string; signature: string },
  fetchImpl: typeof fetch = fetch
): Promise<VerifyResult> {
  const url = `${normaliseBaseUrl(apiUrl)}/v1/auth/verify`
  let response: Response
  try {
    response = await fetchImpl(url, {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
  } catch (cause) {
    throw new SiweApiError(
      `network error verifying signature: ${(cause as Error)?.message ?? cause}`,
      null
    )
  }
  if (!response.ok) {
    throw new SiweApiError(await readMessage(response), response.status)
  }
  const data = (await response.json()) as {
    token?: unknown
    expires_at?: unknown
    address?: unknown
  }
  if (
    typeof data.token !== 'string' ||
    typeof data.expires_at !== 'number' ||
    typeof data.address !== 'string'
  ) {
    throw new SiweApiError('verify response malformed', response.status)
  }
  return { token: data.token, expiresAt: data.expires_at, address: data.address as Address }
}
